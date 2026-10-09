"""Embedded Stage Schema v1 (JSON Schema Draft 2020-12): source of truth for structure."""

from __future__ import annotations

from typing import Any, Sequence

from .constants import (
    ID_PATTERN, LOC_KEY_PATTERN, MAX_WORLD_AXIS, PALETTE_KEY_PATTERN, STAGE_SCHEMA_VERSION,
)


def _build_stage_schema() -> dict[str, Any]:
    def ref(name: str) -> dict[str, Any]:
        return {"$ref": f"#/$defs/{name}"}

    def obj(props: dict[str, Any], required: Sequence[str] = (),
            extra: bool = False) -> dict[str, Any]:
        schema: dict[str, Any] = {"type": "object", "properties": props,
                                  "additionalProperties": extra}
        if required:
            schema["required"] = list(required)
        return schema

    def arr(items: Any, **kw: Any) -> dict[str, Any]:
        return {"type": "array", "items": items, **kw}

    boolean = {"type": "boolean"}
    string = {"type": "string"}
    free_object = {"type": "object"}
    ident, loc = ref("id"), ref("loc_key")
    vec3n, vec3i, region = ref("vec3_num"), ref("vec3_int"), ref("region")
    nn_num, nn_int, pos_num = ref("nonneg_number"), ref("nonneg_int"), ref("pos_number")

    defs = {
        "id": {"type": "string", "pattern": ID_PATTERN, "maxLength": 128},
        "loc_key": {"type": "string", "pattern": LOC_KEY_PATTERN, "maxLength": 256},
        "relpath": {"type": "string", "minLength": 1, "maxLength": 512},
        "vec3_num": arr({"type": "number"}, minItems=3, maxItems=3),
        "vec3_int": arr({"type": "integer"}, minItems=3, maxItems=3),
        "vec3_size": arr({"type": "integer", "minimum": 1, "maximum": MAX_WORLD_AXIS},
                         minItems=3, maxItems=3),
        "region": obj({"min": vec3i, "max": vec3i}, required=("min", "max")),
        "nonneg_number": {"type": "number", "minimum": 0},
        "nonneg_int": {"type": "integer", "minimum": 0},
        "pos_number": {"type": "number", "exclusiveMinimum": 0},
        "number_map": {"type": "object", "additionalProperties": {"type": "number"}},
        "count_map": {"type": "object", "propertyNames": ident,
                      "additionalProperties": nn_int},
        "transform": obj({
            "position": vec3n, "rotation": vec3n,
            "scale": {"anyOf": [vec3n, pos_num]},
        }),
    }
    relpath = ref("relpath")

    stage = obj({
        "id": ident, "title": loc, "description": loc,
        "difficulty": {"enum": ["tutorial", "easy", "medium", "hard", "expert"]},
        "level": {"type": "integer", "minimum": 1},
        "requirements": obj({
            "player_level": {"type": "integer", "minimum": 1},
            "stages": arr(ident, uniqueItems=True),
        }),
        "tags": arr(string),
        "metadata": free_object,
    }, required=("id", "title"))

    story = obj({"dialogue": arr(obj({
        "id": ident, "speaker": string, "text": loc, "condition": free_object,
    }, required=("id", "speaker", "text")))})

    learning = obj({
        "question": loc,
        "curriculum": obj({
            "grade": obj({"min": {"type": "integer", "minimum": 7, "maximum": 9},
                          "max": {"type": "integer", "minimum": 7, "maximum": 9}},
                         required=("min", "max")),
            "topics": arr(ident, minItems=1, uniqueItems=True),
        }, required=("grade", "topics")),
        "lesson": obj({"markdown": relpath}),
        "tutorial_stage": obj({"enabled": boolean, "stage_id": ident}),
        "solution": obj({
            "explanation": loc, "markdown": relpath,
            "demonstration": obj({
                "enabled": boolean, "replay_tasks": boolean,
                "show_calculations": boolean, "show_construction": boolean,
                "simulate_physics": boolean,
            }),
        }),
        "hints": arr(obj({"id": ident, "text": loc, "markdown": relpath},
                         required=("id", "text"))),
    }, required=("question", "curriculum"))

    player = obj({
        "spawn": obj({"position": vec3n, "rotation": vec3n}),
        "initial_state": free_object,
    })
    camera = obj({"mode": {"enum": ["orthographic"]}, "position": vec3n,
                  "target": vec3n, "zoom": pos_num})

    world = obj({
        "bounds": obj({"size": ref("vec3_size"), "origin": vec3i}, required=("size",)),
        "environment": obj({"preset": string, "weather": string,
                            "time_of_day": string, "seed": {"type": "integer"}}),
        "terrain": obj({"type": string, "preset": string, "height": {"type": "number"}}),
        "source": obj({
            "voxel_file": relpath,
            "palette": {"type": "object",
                        "propertyNames": {"type": "string", "pattern": PALETTE_KEY_PATTERN},
                        "additionalProperties": ident},
            "default_block": ident,
            "offset": vec3i,
        }),
        "initial_structure": obj({"blocks": arr(obj({
            "id": ident, "block": ident, "position": vec3i,
            "rotation": vec3i, "state": free_object,
        }, required=("id", "block", "position")))}),
    }, required=("bounds",))

    assets = obj({"custom": arr(obj({
        "id": ident, "file": relpath,
        "type": {"enum": ["environment", "prop", "construction_piece",
                          "structure", "machinery", "interactive"]},
        "transform": ref("transform"),
        "behavior": obj({"placeable": boolean, "removable": boolean,
                         "mass_kg": nn_num, "structural_role": string,
                         "collider": string, "snap_profile": string}),
    }, required=("id", "file", "type")))})

    constraints = obj({
        "max_total_blocks": nn_int, "max_height": nn_int,
        "max_depth_below_ground": nn_int,
        "allowed_regions": arr(region), "forbidden_regions": arr(region),
        "foundation": obj({"required": boolean, "max_depth": nn_int,
                           "allowed_roles": arr(string)}),
        "structural_roles": obj({"required": arr(string), "min_support_count": nn_int,
                                 "max_support_count": nn_int,
                                 "require_path_to_ground": boolean}),
        "overhang": obj({"allowed": boolean, "max_length": nn_num}),
        "placement": obj({"require_support": boolean, "allow_floating": boolean,
                          "snap_to_grid": boolean, "rotation_allowed": boolean}),
        "material_limits": {"type": "object", "propertyNames": ident,
                            "additionalProperties": obj({"min": nn_int, "max": nn_int})},
    })
    construction = obj({
        "allowed_blocks": arr(obj({"id": ident, "min_count": nn_int, "max_count": nn_int},
                                  required=("id",))),
        "allowed_tools": arr(obj({"id": ident, "max_durability": nn_num},
                                 required=("id",))),
        "starting_inventory": obj({"blocks": ref("count_map"), "tools": ref("count_map")}),
        "targets": arr(obj({
            "id": ident,
            "type": {"enum": ["block_group", "wall", "floor", "roof", "room", "house",
                              "bridge", "building", "custom"]},
            "region": region, "required_block": ident,
            "dimensions": obj({"width": pos_num, "height": pos_num, "depth": pos_num,
                               "tolerance": nn_num}),
            "required_roles": arr(string),
            "constraints": free_object,
        }, required=("id", "type"))),
        "constraints": constraints,
        "validation": obj({k: boolean for k in (
            "dimensions", "target_geometry", "material_limits",
            "support_graph", "architecture", "block_roles")}),
    })

    economy = obj({
        "budget": nn_num,
        "rules": obj({"allow_overdraft": boolean,
                      "waste_percentage": {"type": "number", "minimum": 0, "maximum": 100},
                      "must_finish_within_budget": boolean}),
        "multipliers": ref("number_map"),
    }, required=("budget",))

    physics = obj({
        "enabled": boolean,
        "gravity": pos_num,
        "structural": obj({"stability_required": boolean, "collapse_enabled": boolean,
                           "ground_support_required": boolean,
                           "max_displacement": nn_num, "max_rotation": nn_num}),
        "tests": arr(obj({
            "id": ident,
            "type": {"enum": ["static_load", "moving_load", "earthquake", "wind", "impact"]},
            "duration_seconds": nn_num,
            "load": obj({
                "mass_kg": nn_num, "force_n": nn_num,
                "target": obj({
                    "type": {"enum": ["point", "surface", "region", "structure", "body"]},
                    "id": ident, "position": vec3n, "region": region,
                }),
                "distribution": {"enum": ["point", "uniform", "moving_point",
                                          "uniform_surface"]},
                "direction": vec3n,
            }),
            "environment": free_object, "acceptance": free_object,
        }, required=("id", "type"))),
    }, required=("enabled",))

    objectives = obj({
        "tasks": arr(obj({
            "id": ident,
            "type": {"enum": ["math", "construction", "purchase", "inspection",
                              "dialogue", "custom"]},
            "depends_on": arr(ident),
            "optional": boolean,
            "math": obj({
                "type": ident, "generator": free_object, "expression": string,
                "answer": obj({
                    "mode": {"enum": ["exact", "tolerance", "expression",
                                      "multiple_choice", "custom"]},
                    "value": {}, "tolerance": nn_num, "expression": string,
                }, required=("mode",)),
            }, required=("type",)),
            "target": ident, "data": free_object,
        }, required=("id", "type")), minItems=1),
        "completion": free_object,
        "failure": free_object,
        "checkpoints": arr(obj({"id": ident, "after_task": ident, "autosave": boolean},
                               required=("id",))),
        "time_limit_seconds": nn_num,
        "scoring": obj({"enabled": boolean, "maximum": nn_num,
                        "criteria": ref("number_map")}),
    }, required=("tasks", "completion"))

    events = arr(obj({
        "id": ident,
        "trigger": obj({"type": {"type": "string", "minLength": 1},
                        "condition": free_object}, required=("type",)),
        "actions": arr(free_object),
    }, required=("id", "trigger", "actions")))

    id_list = arr(ident)
    rewards = obj({
        "money": nn_num, "xp": nn_int,
        "unlocks": obj({"blocks": id_list, "tools": id_list,
                        "stages": id_list, "assets": id_list}),
    })
    audio = obj({"music": ident, "ambience": ident,
                 "sounds": {"type": "object", "additionalProperties": ident}})
    localization = obj({"ftl": relpath, "namespace": loc})

    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "OopsMath Stage Schema v1",
        "type": "object",
        "$defs": defs,
        "properties": {
            "schema_version": {"type": "integer", "const": STAGE_SCHEMA_VERSION},
            "stage": stage, "story": story, "learning": learning, "player": player,
            "camera": camera, "world": world, "assets": assets,
            "construction": construction, "economy": economy, "physics": physics,
            "objectives": objectives, "events": events, "rewards": rewards,
            "audio": audio, "localization": localization,
            "extensions": free_object,  # future extension namespace, preserved verbatim
        },
        "required": ["schema_version", "stage", "learning", "world",
                     "construction", "objectives", "rewards"],
        "additionalProperties": False,
    }


STAGE_SCHEMA: dict[str, Any] = _build_stage_schema()
