"""
Layout: stage list on the left; Report / Stage / DAT tabs on the right.
Keys: v validate | b build | a build all | i inspect | r rescan | s strict | q quit
"""

from __future__ import annotations

import dataclasses
from pathlib import Path
from typing import Any, Optional

from rich.markup import escape
from rich.text import Text
from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.widgets import (
    Footer, Header, Label, OptionList, RichLog, Static, TabbedContent, TabPane,
)
from textual.widgets.option_list import Option

from . import __version__
from .cli import dump_section
from .constants import COMPRESSION_NONE, COMPRESSION_ZSTD, HeaderFlag
from .dat import ParsedDat
from .registry import CompileOptions
from .ui import human_size
from .workflow import (
    BuildOutcome, build_one, discover_stage_dirs, read_dat, with_known_stage_ids,
)

# status symbols shown in the stage list
_PENDING, _VALID, _FAILED, _BUILT = "pending", "valid", "failed", "built"
_ICONS = {_PENDING: ("\u25cb", "grey50"), _VALID: ("\u2714", "green"),
          _FAILED: ("\u2718", "red"), _BUILT: ("\u25cf", "cyan")}


class OmscApp(App[None]):
    TITLE = f"OMSC {__version__}"
    SUB_TITLE = "OopsMath Stage Compiler"

    CSS = """
    #sidebar { width: 36; border-right: solid $primary-darken-2; }
    #sidebar > Label { padding: 0 1; background: $primary-darken-2; width: 100%; }
    #stages { height: 1fr; border: none; }
    #status { height: 3; padding: 0 1; border-top: solid $primary-darken-2; }
    RichLog { padding: 0 1; }
    #dat-header { height: auto; padding: 0 1; }
    #sections { width: 22; border-right: solid $primary-darken-2; }
    """

    BINDINGS = [
        Binding("v", "validate", "Validate"),
        Binding("b", "build", "Build"),
        Binding("a", "build_all", "Build all"),
        Binding("i", "inspect", "Inspect DAT"),
        Binding("r", "rescan", "Rescan"),
        Binding("s", "toggle_strict", "Strict"),
        Binding("q", "quit", "Quit"),
    ]

    def __init__(self, root: Path, opts: CompileOptions, out_dir: Path) -> None:
        super().__init__()
        self.root = root
        self.base_opts = opts
        self.opts = opts
        self.out_dir = out_dir
        self.dirs: list[Path] = []
        self.status: dict[Path, str] = {}
        self.outcomes: dict[Path, BuildOutcome] = {}
        self.dats: dict[Path, Path] = {}
        self.current: Optional[Path] = None
        self.parsed: Optional[ParsedDat] = None

    # ----------------------------------------------------------------- layout

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal():
            with Vertical(id="sidebar"):
                yield Label("Stages")
                yield OptionList(id="stages")
                yield Static("", id="status")
            with TabbedContent(initial="tab-report"):
                with TabPane("Report", id="tab-report"):
                    yield RichLog(id="report", markup=True, wrap=True)
                with TabPane("Stage", id="tab-stage"):
                    yield RichLog(id="summary", markup=True, wrap=True)
                with TabPane("DAT", id="tab-dat"):
                    with Vertical():
                        yield Static("Build a stage (b) to inspect its package.", id="dat-header")
                        with Horizontal():
                            yield OptionList(id="sections")
                            yield RichLog(id="dump", markup=False, wrap=False, highlight=True)
        yield Footer()

    def on_mount(self) -> None:
        self.action_rescan()

    # ------------------------------------------------------------------ helpers

    def _set_status_line(self) -> None:
        strict = "on" if self.opts.strict else "off"
        built = sum(1 for s in self.status.values() if s == _BUILT)
        failed = sum(1 for s in self.status.values() if s == _FAILED)
        self.query_one("#status", Static).update(
            f"strict: {strict}\n{len(self.dirs)} stage(s), {built} built, {failed} failed")

    def _label(self, path: Path) -> Text:
        icon, color = _ICONS[self.status.get(path, _PENDING)]
        try:
            name = path.relative_to(self.root).as_posix() if path != self.root else path.name
        except ValueError:
            name = path.name
        return Text.assemble((f"{icon} ", color), name or str(path))

    def _refresh_row(self, path: Path) -> None:
        if path in self.dirs:
            self.query_one("#stages", OptionList).replace_option_prompt_at_index(
                self.dirs.index(path), self._label(path))
        self._set_status_line()

    def _set_status(self, path: Path, status: str) -> None:
        self.status[path] = status
        self._refresh_row(path)

    def _log(self, widget_id: str, *lines: str, clear: bool = False) -> None:
        log = self.query_one(widget_id, RichLog)
        if clear:
            log.clear()
        for line in lines:
            log.write(line)

    # ------------------------------------------------------------------ actions

    def action_rescan(self) -> None:
        self.dirs = discover_stage_dirs(self.root) if self.root.is_dir() else []
        self.opts, _ids, _counts = with_known_stage_ids(self.dirs, self.base_opts)
        self.status = {d: self.status.get(d, _PENDING) for d in self.dirs}
        stages = self.query_one("#stages", OptionList)
        stages.clear_options()
        stages.add_options([Option(self._label(d)) for d in self.dirs])
        self._set_status_line()
        if not self.dirs:
            self._log("#report", f"[yellow]No stages (stage.yaml) found under {escape(str(self.root))}[/]",
                      clear=True)
            self.current = None
            return
        stages.highlighted = 0
        self.current = self.dirs[0]
        self.action_validate()

    def action_toggle_strict(self) -> None:
        self.base_opts = dataclasses.replace(self.base_opts, strict=not self.base_opts.strict)
        self.opts = dataclasses.replace(self.opts, strict=self.base_opts.strict)
        self.notify(f"Strict mode {'on' if self.opts.strict else 'off'}")
        self.action_validate()

    def action_validate(self) -> None:
        path = self.current
        if path is None:
            return
        self._run(lambda: build_one(path, None, self.opts, write=False),
                  lambda outcome: self._show_validation(path, outcome, built=False))

    def action_build(self) -> None:
        path = self.current
        if path is None:
            return
        self._run(lambda: build_one(path, self.out_dir, self.opts, output_is_dir=True),
                  lambda outcome: self._show_validation(path, outcome, built=True))

    def action_build_all(self) -> None:
        dirs, opts, out_dir = list(self.dirs), self.opts, self.out_dir

        def work() -> list[tuple[Path, BuildOutcome]]:
            return [(d, build_one(d, out_dir, opts, output_is_dir=True)) for d in dirs]

        def done(results: list[tuple[Path, BuildOutcome]]) -> None:
            for path, outcome in results:
                self._record(path, outcome, built=True)
            ok = sum(1 for _p, o in results if o.ok)
            self.notify(f"Built {ok} of {len(results)} stage(s)",
                        severity="information" if ok == len(results) else "error")
            if self.current is not None and self.current in self.outcomes:
                self._render_report(self.current, self.outcomes[self.current])

        self._run(work, done)

    def action_inspect(self) -> None:
        path = self.current
        if path is None or path not in self.dats:
            self.notify("Build the stage first (b)", severity="warning")
            return
        self._show_dat(self.dats[path])
        self.query_one(TabbedContent).active = "tab-dat"

    # --------------------------------------------------------------- worker glue

    def _run(self, work, done) -> None:
        def runner() -> None:
            try:
                result = work()
            except Exception as exc:  # surface unexpected failures in the UI
                self.call_from_thread(self._fail, exc)
                return
            self.call_from_thread(done, result)

        self.run_worker(runner, thread=True, exclusive=True, group="omsc")

    def _fail(self, exc: Exception) -> None:
        self._log("#report", f"[red bold]Unexpected error:[/] {escape(str(exc))}", clear=True)
        self.notify(str(exc), severity="error")

    # ---------------------------------------------------------------- rendering

    def _record(self, path: Path, outcome: BuildOutcome, *, built: bool) -> None:
        self.outcomes[path] = outcome
        if outcome.output is not None:
            self.dats[path] = outcome.output
        if not outcome.ok:
            self._set_status(path, _FAILED)
        else:
            self._set_status(path, _BUILT if built else _VALID)

    def _show_validation(self, path: Path, outcome: BuildOutcome, *, built: bool) -> None:
        self._record(path, outcome, built=built)
        if path == self.current:
            self._render_report(path, outcome)
            if built and outcome.output is not None:
                self._show_dat(outcome.output)

    def _render_report(self, path: Path, outcome: BuildOutcome) -> None:
        log = self.query_one("#report", RichLog)
        log.clear()
        log.write(f"[b cyan]{escape(str(path))}[/]")
        if outcome.skipped_reason:
            log.write(f"[red bold]ERROR[/] {escape(outcome.skipped_reason)}")
            return
        result = outcome.result
        assert result is not None
        for line in result.passed:
            log.write(f"[green]\u2714[/] {escape(line)}")
        for warning in result.diag.warnings:
            log.write(f"[yellow bold]WARNING[/] {escape(warning)}")
        for error in result.diag.errors:
            log.write(f"[red bold]ERROR[/] {escape(error)}")
        if outcome.error:
            log.write(f"[red bold]ERROR[/] {escape(outcome.error)}")
        if outcome.output is not None:
            log.write(f"[green]\u2714 Stage compiled[/] -> {escape(str(outcome.output))} "
                      f"[dim]({human_size(outcome.size)})[/]")
        counts = f"{len(result.diag.errors)} error(s), {len(result.diag.warnings)} warning(s)"
        log.write(f"[green bold]Result: valid ({counts})[/]" if result.diag.ok
                  else f"[red bold]Result: FAILED ({counts})[/]")
        self._render_summary(result.source.data if result.source else None)

    def _render_summary(self, data: Optional[dict[str, Any]]) -> None:
        log = self.query_one("#summary", RichLog)
        log.clear()
        if data is None:
            log.write("[dim]Fix the errors in the Report tab to see the stage summary.[/]")
            return
        stage, learning = data["stage"], data["learning"]
        grade = learning["curriculum"]["grade"]
        tasks = data["objectives"]["tasks"]
        rows = [
            ("id", stage["id"]), ("title key", stage["title"]),
            ("difficulty", stage.get("difficulty", "-")), ("level", stage.get("level", "-")),
            ("grade", f"{grade['min']}-{grade['max']}"),
            ("topics", ", ".join(learning["curriculum"]["topics"])),
            ("world size", "x".join(map(str, data["world"]["bounds"]["size"]))),
            ("tasks", f"{len(tasks)} ({sum(1 for t in tasks if t['type'] == 'math')} math)"),
            ("targets", len(data["construction"].get("targets", []))),
            ("blocks", ", ".join(b["id"] for b in data["construction"].get("allowed_blocks", [])) or "-"),
            ("budget", data.get("economy", {}).get("budget", "-")),
            ("physics tests", len(data.get("physics", {}).get("tests", []))),
            ("events", len(data.get("events", []))),
            ("rewards", f"{data['rewards'].get('money', 0)} money, {data['rewards'].get('xp', 0)} xp"),
        ]
        for label, value in rows:
            log.write(f"[b]{label:<14}[/] {escape(str(value))}")

    def _show_dat(self, dat_path: Path) -> None:
        header = self.query_one("#dat-header", Static)
        sections = self.query_one("#sections", OptionList)
        sections.clear_options()
        self.query_one("#dump", RichLog).clear()
        try:
            self.parsed = parsed = read_dat(dat_path)
        except Exception as exc:
            self.parsed = None
            header.update(f"[red]Cannot read {escape(str(dat_path))}: {escape(str(exc))}[/]")
            return
        h = parsed.header
        flags = " | ".join(f.name for f in HeaderFlag if h.flags & int(f)) or "none"
        state = "[green]valid[/]" if parsed.is_valid else "[red bold]INVALID[/]"
        header.update(f"[b]{escape(str(dat_path))}[/]  {human_size(h.file_size)}  {state}\n"
                      f"format v{h.format_version}  schema v{h.schema_version}  "
                      f"{h.section_count} sections  flags: {flags}")
        for s in parsed.sections:
            comp = {COMPRESSION_NONE: "raw", COMPRESSION_ZSTD: "zstd"}.get(s.compression, "?")
            mark = "\u2714" if s.status == "OK" else "\u2718"
            sections.add_option(Option(f"{mark} {s.type}  {human_size(s.stored_size)} {comp}"))
        if parsed.sections:
            sections.highlighted = 0

    # ------------------------------------------------------------------- events

    @on(OptionList.OptionHighlighted, "#stages")
    def _stage_highlighted(self, event: OptionList.OptionHighlighted) -> None:
        if 0 <= event.option_index < len(self.dirs):
            path = self.dirs[event.option_index]
            if path == self.current and path in self.outcomes:
                return
            self.current = path
            if path in self.outcomes:
                self._render_report(path, self.outcomes[path])
                if path in self.dats:
                    self._show_dat(self.dats[path])
            else:
                self.action_validate()

    @on(OptionList.OptionHighlighted, "#sections")
    def _section_highlighted(self, event: OptionList.OptionHighlighted) -> None:
        dump = self.query_one("#dump", RichLog)
        dump.clear()
        if self.parsed is None or not 0 <= event.option_index < len(self.parsed.sections):
            return
        section = self.parsed.sections[event.option_index]
        if section.raw is None:
            dump.write(f"<unreadable: {section.status}>")
            return
        try:
            dump.write(dump_section(section.type, section.raw))
        except Exception as exc:
            dump.write(f"<cannot decode {section.type}: {exc}>")


def run_tui(root: Path, opts: CompileOptions, out_dir: Path) -> int:
    OmscApp(root, opts, out_dir).run()
    return 0
