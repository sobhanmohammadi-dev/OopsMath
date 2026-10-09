"""Validation layer 3: IDs, cross-references, conditions and registries."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Sequence

from ..constants import AIR_BLOCK, COMPARE_OPERATORS, CONDITION_FORMS, MAX_CONDITION_DEPTH
from ..errors import Diagnostics
from ..registry import CompileOptions, Registry


@dataclass
class RefIndex:
    task_ids: set[str]
    target_ids: set[str]
    test_ids: set[str]


def _unique_ids(items: Sequence[Mapping[str, Any]], path: str, label: str,
                diag: Diagnostics) -> set[str]:
    first: dict[str, int] = {}
    for i, item in enumerate(items):
        ident = item["id"]
        if ident in first:
            diag.error(f"{path}[{i}].id duplicates {label} id '{ident}' "
                       f"(first declared at {path}[{first[ident]}])")
        else:
            first[ident] = i
    return set(first)


def _check_region(region: Mapping[str, Any], path: str, diag: Diagnostics) -> None:
    for axis, name in enumerate("xyz"):
        if region["min"][axis] > region["max"][axis]:
            diag.error(f"{path} has min greater than max on the {name} axis")


def validate_condition(cond: Any, path: str, index: RefIndex, diag: Diagnostics,
                       depth: int = 0) -> None:
    """Validate the structure and references of a runtime condition (never evaluates it)."""
    if depth > MAX_CONDITION_DEPTH:
        diag.error(f"{path}: condition is nested deeper than {MAX_CONDITION_DEPTH} levels")
        return
    if not isinstance(cond, dict):
        diag.error(f"{path}: a condition must be an object")
        return
    if len(cond) != 1:
        diag.error(f"{path}: a condition must contain exactly one form "
                   f"({', '.join(CONDITION_FORMS)}), found {sorted(cond)}")
        return
    ((form, arg),) = cond.items()
    here = f"{path}.{form}"
    if form in ("all", "any"):
        if not isinstance(arg, list) or not arg:
            diag.error(f"{here} must be a non-empty list of conditions")
            return
        for i, sub in enumerate(arg):
            validate_condition(sub, f"{here}[{i}]", index, diag, depth + 1)
    elif form == "not":
        validate_condition(arg, here, index, diag, depth + 1)
    elif form in ("task_completed", "target_completed", "physics_test_passed"):
        known, label = {"task_completed": (index.task_ids, "task"),
                        "target_completed": (index.target_ids, "construction target"),
                        "physics_test_passed": (index.test_ids, "physics test")}[form]
        if not isinstance(arg, str):
            diag.error(f"{here} must be an ID string")
        elif arg not in known:
            diag.error(f"{here} references unknown {label} '{arg}'")
    elif form == "structure_stable":
        if not isinstance(arg, (bool, dict)):
            diag.error(f"{here} must be a boolean or an object")
    elif form == "compare":
        if not isinstance(arg, dict):
            diag.error(f"{here} must be an object with metric, operator and value")
            return
        extra = sorted(set(arg) - {"metric", "operator", "value"})
        if extra:
            diag.error(f"{here} has unexpected properties {extra}")
        if not isinstance(arg.get("metric"), str) or not arg.get("metric"):
            diag.error(f"{here}.metric must be a non-empty string")
        if arg.get("operator") not in COMPARE_OPERATORS:
            diag.error(f"{here}.operator must be one of {', '.join(COMPARE_OPERATORS)}")
        if "value" not in arg:
            diag.error(f"{here}.value is required")
    else:
        diag.error(f"{path}: unknown condition form '{form}' "
                   f"(expected one of {', '.join(CONDITION_FORMS)})")


def _check_registered(kind: str, known: Any, value: str, path: str, diag: Diagnostics) -> None:
    if known is not None and value not in known:
        diag.error(f"{path} references unknown {kind} '{value}'")


check_registered = _check_registered  # public alias for sibling modules


def _task_cycle(tasks: Sequence[Mapping[str, Any]]) -> list[str]:
    """Return task IDs that sit on (or behind) a dependency cycle (Kahn's algorithm)."""
    ids = {t["id"] for t in tasks}
    pending = {t["id"]: {d for d in t.get("depends_on", []) if d in ids and d != t["id"]}
               for t in tasks}
    progressed = True
    while progressed and pending:
        ready = [tid for tid, deps in pending.items() if not deps]
        progressed = bool(ready)
        for tid in ready:
            del pending[tid]
        for deps in pending.values():
            deps.difference_update(ready)
    return sorted(pending)


def _check_math_answer(math: Mapping[str, Any], path: str, diag: Diagnostics) -> None:
    answer = math.get("answer")
    if answer is None:
        return
    mode, generated = answer["mode"], "generator" in math
    if mode in ("exact", "tolerance", "multiple_choice") and "value" not in answer and not generated:
        diag.error(f"{path}.answer.value is required for mode '{mode}' without a generator")
    if mode == "tolerance" and "tolerance" not in answer and not generated:
        diag.error(f"{path}.answer.tolerance is required for mode 'tolerance' without a generator")
    if mode == "expression" and "expression" not in answer and "expression" not in math:
        diag.error(f"{path}.answer.expression (or {path}.expression) is required for mode 'expression'")


def validate_references(data: dict[str, Any], opts: CompileOptions, diag: Diagnostics) -> None:
    reg = opts.manifest or Registry()
    stage, learning, world = data["stage"], data["learning"], data["world"]
    construction, objectives, rewards = data["construction"], data["objectives"], data["rewards"]
    stage_id = stage["id"]

    _unique_ids(data.get("story", {}).get("dialogue", []), "story.dialogue", "dialogue", diag)
    _unique_ids(learning.get("hints", []), "learning.hints", "hint", diag)
    instances = world.get("initial_structure", {}).get("blocks", [])
    _unique_ids(instances, "world.initial_structure.blocks", "block instance", diag)
    custom = data.get("assets", {}).get("custom", [])
    asset_ids = _unique_ids(custom, "assets.custom", "asset", diag)
    targets = construction.get("targets", [])
    tests = data.get("physics", {}).get("tests", [])
    index = RefIndex(
        task_ids=_unique_ids(objectives["tasks"], "objectives.tasks", "task", diag),
        target_ids=_unique_ids(targets, "construction.targets", "target", diag),
        test_ids=_unique_ids(tests, "physics.tests", "physics test", diag))
    _unique_ids(objectives.get("checkpoints", []), "objectives.checkpoints", "checkpoint", diag)
    _unique_ids(data.get("events", []), "events", "event", diag)
    allowed_blocks = construction.get("allowed_blocks", [])
    allowed_tools = construction.get("allowed_tools", [])
    allowed_block_ids = _unique_ids(allowed_blocks, "construction.allowed_blocks",
                                    "allowed block", diag)
    allowed_tool_ids = _unique_ids(allowed_tools, "construction.allowed_tools",
                                   "allowed tool", diag)

    # --- stage ID references ------------------------------------------------
    def stage_ref(value: str, path: str, forbid_self: bool) -> None:
        if value == stage_id:
            (diag.error if forbid_self else diag.warn)(f"{path} references the stage itself ('{value}')")
        elif opts.known_stage_ids is not None and value not in opts.known_stage_ids:
            diag.warn(f"{path} references stage '{value}' which was not found among the stages being built")

    for i, ref_id in enumerate(stage.get("requirements", {}).get("stages", [])):
        stage_ref(ref_id, f"stage.requirements.stages[{i}]", True)
    tutorial = learning.get("tutorial_stage", {})
    if tutorial.get("enabled") and "stage_id" not in tutorial:
        diag.error("learning.tutorial_stage.stage_id is required when the tutorial stage is enabled")
    if "stage_id" in tutorial:
        stage_ref(tutorial["stage_id"], "learning.tutorial_stage.stage_id", True)
    unlocks = rewards.get("unlocks", {})
    for i, ref_id in enumerate(unlocks.get("stages", [])):
        stage_ref(ref_id, f"rewards.unlocks.stages[{i}]", False)

    grade = learning["curriculum"]["grade"]
    if grade["min"] > grade["max"]:
        diag.error("learning.curriculum.grade.min must not exceed grade.max")

    # --- world ---------------------------------------------------------------
    bounds = world["bounds"]
    size, origin = bounds["size"], bounds.get("origin", [0, 0, 0])
    seen_positions: dict[tuple[int, ...], int] = {}
    for i, block in enumerate(instances):
        base = f"world.initial_structure.blocks[{i}]"
        pos = tuple(block["position"])
        if block["block"] == AIR_BLOCK:
            diag.error(f"{base}.block: 'air' is reserved and cannot be placed")
        if any(not origin[a] <= pos[a] < origin[a] + size[a] for a in range(3)):
            diag.error(f"{base}.position {list(pos)} is outside the world bounds")
        if pos in seen_positions:
            diag.error(f"{base}.position {list(pos)} is already used by "
                       f"world.initial_structure.blocks[{seen_positions[pos]}]")
        else:
            seen_positions[pos] = i

    # --- construction --------------------------------------------------------
    for i, entry in enumerate(allowed_blocks):
        if entry.get("min_count", 0) > entry.get("max_count", entry.get("min_count", 0)):
            diag.error(f"construction.allowed_blocks[{i}].min_count exceeds max_count")
    constraints = construction.get("constraints", {})
    for name, limit in constraints.get("material_limits", {}).items():
        if limit.get("min", 0) > limit.get("max", limit.get("min", 0)):
            diag.error(f"construction.constraints.material_limits.{name}: min exceeds max")
    for i, tgt in enumerate(targets):
        if "region" in tgt:
            _check_region(tgt["region"], f"construction.targets[{i}].region", diag)
    for group in ("allowed_regions", "forbidden_regions"):
        for i, region in enumerate(constraints.get(group, [])):
            _check_region(region, f"construction.constraints.{group}[{i}]", diag)

    inventory = construction.get("starting_inventory", {})
    if allowed_blocks:
        for name in inventory.get("blocks", {}):
            if name not in allowed_block_ids:
                diag.warn(f"construction.starting_inventory.blocks.{name} is not listed in allowed_blocks")
        for i, tgt in enumerate(targets):
            rb = tgt.get("required_block")
            if rb and rb not in allowed_block_ids:
                diag.warn(f"construction.targets[{i}].required_block '{rb}' is not listed in allowed_blocks")
    if allowed_tools:
        for name in inventory.get("tools", {}):
            if name not in allowed_tool_ids:
                diag.warn(f"construction.starting_inventory.tools.{name} is not listed in allowed_tools")

    # --- built-in block / tool / asset registries ------------------------------
    block_refs = [(f"construction.allowed_blocks[{i}].id", b["id"]) for i, b in enumerate(allowed_blocks)]
    block_refs += [(f"construction.starting_inventory.blocks.{k}", k) for k in inventory.get("blocks", {})]
    block_refs += [(f"construction.targets[{i}].required_block", t["required_block"])
                   for i, t in enumerate(targets) if "required_block" in t]
    block_refs += [(f"construction.constraints.material_limits.{k}", k)
                   for k in constraints.get("material_limits", {})]
    block_refs += [(f"world.initial_structure.blocks[{i}].block", b["block"]) for i, b in enumerate(instances)]
    block_refs += [(f"rewards.unlocks.blocks[{i}]", b) for i, b in enumerate(unlocks.get("blocks", []))]
    for path, value in block_refs:
        _check_registered("block", reg.blocks, value, path, diag)
    tool_refs = [(f"construction.allowed_tools[{i}].id", t["id"]) for i, t in enumerate(allowed_tools)]
    tool_refs += [(f"construction.starting_inventory.tools.{k}", k) for k in inventory.get("tools", {})]
    tool_refs += [(f"rewards.unlocks.tools[{i}]", t) for i, t in enumerate(unlocks.get("tools", []))]
    for path, value in tool_refs:
        _check_registered("tool", reg.tools, value, path, diag)

    if reg.assets is not None:
        known_assets = reg.assets | asset_ids
        for i, value in enumerate(unlocks.get("assets", [])):
            _check_registered("asset", known_assets, value, f"rewards.unlocks.assets[{i}]", diag)
    audio = data.get("audio", {})
    audio_pool = [p for p in (reg.audio, reg.assets) if p is not None]
    if audio_pool:
        known_audio = frozenset().union(*audio_pool) | asset_ids
        audio_refs = [(f"audio.{k}", audio[k]) for k in ("music", "ambience") if k in audio]
        audio_refs += [(f"audio.sounds.{k}", v) for k, v in audio.get("sounds", {}).items()]
        for path, value in audio_refs:
            _check_registered("audio asset", known_audio, value, path, diag)

    # --- physics ---------------------------------------------------------------
    physics = data.get("physics", {})
    if tests and not physics.get("enabled", False):
        diag.warn("physics.tests are declared but physics.enabled is false")
    for i, test in enumerate(tests):
        load = test.get("load", {})
        target = load.get("target", {})
        base = f"physics.tests[{i}].load.target"
        if "id" in target:
            pool = index.target_ids | asset_ids | {b["id"] for b in instances}
            if target["id"] not in pool:
                (diag.warn if target.get("type") == "body" else diag.error)(
                    f"{base}.id references unknown construction target, asset or block instance '{target['id']}'")
        if "region" in target:
            _check_region(target["region"], f"{base}.region", diag)

    # --- objectives ------------------------------------------------------------
    for i, task in enumerate(objectives["tasks"]):
        base = f"objectives.tasks[{i}]"
        for j, dep in enumerate(task.get("depends_on", [])):
            if dep == task["id"]:
                diag.error(f"{base}.depends_on[{j}] makes the task depend on itself")
            elif dep not in index.task_ids:
                diag.error(f"{base}.depends_on[{j}] references unknown task '{dep}'")
        if "target" in task and task["target"] not in index.target_ids:
            diag.error(f"{base}.target references unknown construction target '{task['target']}'")
        if task["type"] == "construction" and "target" not in task:
            diag.warn(f"{base} is a construction task without a target")
        math = task.get("math")
        if task["type"] == "math" and math is None:
            diag.error(f"{base}.math is required for math tasks")
        elif math is not None:
            if task["type"] != "math":
                diag.warn(f"{base}.math is declared on a '{task['type']}' task")
            _check_math_answer(math, f"{base}.math", diag)
    cyc = _task_cycle(objectives["tasks"])
    if cyc:
        diag.error(f"objectives.tasks contain a dependency cycle involving: {', '.join(cyc)}")
    for i, cp in enumerate(objectives.get("checkpoints", [])):
        if "after_task" in cp and cp["after_task"] not in index.task_ids:
            diag.error(f"objectives.checkpoints[{i}].after_task references unknown task '{cp['after_task']}'")

    validate_condition(objectives["completion"], "objectives.completion", index, diag)
    if "failure" in objectives:
        validate_condition(objectives["failure"], "objectives.failure", index, diag)
    for i, event in enumerate(data.get("events", [])):
        if "condition" in event["trigger"]:
            validate_condition(event["trigger"]["condition"],
                               f"events[{i}].trigger.condition", index, diag)
    for i, line in enumerate(data.get("story", {}).get("dialogue", [])):
        if "condition" in line:
            validate_condition(line["condition"], f"story.dialogue[{i}].condition", index, diag)
