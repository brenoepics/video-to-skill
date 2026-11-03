"""The recipe is the contract, so these tests are mostly about refusal.

A recipe that loads but means something slightly different from what its
author intended would silently change the gold labels, and nothing
downstream would notice -- so every structural rule gets a test that
proves the loader says no.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from v2sbench.render.recipe import (
    TERMINAL_ROWS,
    Recipe,
    RecipeError,
    build_recipe,
    load_recipe,
    validate_against_schema,
)

FIXTURE = Path(__file__).parent / "fixtures" / "recipe_sample.json"


def minimal(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "schema_version": 1,
        "id": "demo",
        "title": "Demo",
        "width": 1280,
        "height": 720,
        "fps": 12,
        "duration_secs": 9.0,
        "stills": [
            {"t": 0.0, "terminal": {}},
            {"t": 2.0, "step": "01-init", "terminal": {"type": "git init"}},
            {"t": 5.0, "step": "01-init", "terminal": {"output": ["Initialized"]}},
            {"t": 7.0, "continuous": True, "terminal": {"output": ["more"]}},
        ],
    }
    base.update(overrides)
    return base


def test_the_fixture_loads_and_matches_its_own_header() -> None:
    recipe = load_recipe(FIXTURE)
    raw = json.loads(FIXTURE.read_text())
    assert (recipe.id, recipe.width, recipe.height, recipe.fps) == (
        raw["id"], raw["width"], raw["height"], raw["fps"],
    )
    assert len(recipe.stills) == len(raw["stills"])
    assert recipe.frame_count == round(raw["duration_secs"] * raw["fps"]) == 240
    assert recipe.source == FIXTURE


def test_the_fixture_is_checked_against_the_published_schema() -> None:
    pytest.importorskip("jsonschema")
    assert validate_against_schema(json.loads(FIXTURE.read_text()), source=FIXTURE) is True


def test_the_schema_rejects_a_field_it_does_not_define() -> None:
    pytest.importorskip("jsonschema")
    data = minimal()
    data["stills"][0]["zoom"] = 2
    with pytest.raises(RecipeError, match="schema"):
        validate_against_schema(data)


def test_durations_run_to_the_next_still_and_the_last_to_the_end() -> None:
    recipe = build_recipe(minimal())
    assert recipe.still_durations() == [2.0, 3.0, 2.0, 2.0]
    assert sum(recipe.still_durations()) == recipe.duration_secs


def test_continuous_stills_are_not_gold_visual_changes() -> None:
    recipe = build_recipe(minimal())
    assert recipe.change_times() == [2.0, 5.0]  # 7.0 is marked continuous
    assert recipe.steps() == [("01-init", 2.0, 7.0)]


def test_the_terminal_is_a_state_machine_across_stills() -> None:
    recipe = build_recipe(minimal())
    assert recipe.stills[0].screen.lines == ("$ ",)
    assert recipe.stills[1].screen.lines == ("$ git init",)
    assert recipe.stills[2].screen.lines == ("$ git init", "Initialized", "$ ")
    assert recipe.stills[3].screen.lines == ("$ git init", "Initialized", "more", "$ ")


def test_a_prompt_change_applies_before_the_command_typed_beside_it() -> None:
    data = minimal(stills=[
        {"t": 0.0, "terminal": {}},
        {"t": 2.0, "terminal": {"prompt": "demo@host:~$ ", "type": "uptime"}},
    ])
    assert build_recipe(data).stills[1].screen.lines == ("demo@host:~$ uptime",)


def test_lines_replace_the_whole_screen_and_the_screen_scrolls() -> None:
    many = [f"line {i}" for i in range(TERMINAL_ROWS + 5)]
    data = minimal(stills=[
        {"t": 0.0, "terminal": {"type": "ls"}},
        {"t": 2.0, "terminal": {"lines": many}},
    ])
    shown = build_recipe(data).stills[1].screen.lines
    assert len(shown) == TERMINAL_ROWS
    assert shown[-1] == many[-1]
    assert "$ ls" not in shown


def test_typing_exposes_every_prefix_of_the_command() -> None:
    recipe = build_recipe(minimal())
    typing = recipe.stills[1].typing
    assert typing is not None and typing.typed == "git init"
    assert typing.at(0).lines == ("$ ",)
    assert typing.at(4).lines == ("$ git ",)
    assert typing.at(len(typing.typed)) == recipe.stills[1].screen


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ({"stills": [{"t": 0.5, "terminal": {}}, {"t": 2.0, "terminal": {}}]}, "increase"),
        ({"stills": [{"t": 0.0, "terminal": {}}, {"t": 0.0, "terminal": {}}]}, "increase"),
        ({"stills": [{"t": 0.0, "terminal": {}}, {"t": 99.0, "terminal": {}}]}, "after the end"),
        ({"stills": [{"t": 0.0, "terminal": {}, "slide": {}},
                     {"t": 1.0, "slide": {}}]}, "exactly one"),
        ({"stills": [{"t": 0.0}, {"t": 1.0, "slide": {}}]}, "exactly one"),
        ({"stills": [{"t": 0.0, "terminal": {}}]}, "at least two"),
        ({"schema_version": 2}, "schema_version"),
        ({"fps": 0}, "positive"),
        ({"width": 1281}, "odd side"),
    ],
)
def test_a_broken_recipe_is_refused_with_a_reason(mutation: dict[str, Any], message: str) -> None:
    with pytest.raises(RecipeError, match=message):
        build_recipe(minimal(**mutation))


def test_a_step_label_cannot_come_back_after_another_step() -> None:
    data = minimal(stills=[
        {"t": 0.0, "step": "a", "terminal": {}},
        {"t": 1.0, "step": "b", "terminal": {}},
        {"t": 2.0, "step": "a", "terminal": {}},
    ])
    with pytest.raises(RecipeError, match="two separate runs"):
        build_recipe(data)


@pytest.mark.parametrize(
    "cues",
    [
        [{"t": 2.0, "text": "second"}, {"t": 1.0, "text": "first"}],
        [{"t": 99.0, "text": "past the end"}],
        [{"t": 1.0, "text": "   "}],
    ],
)
def test_narration_cues_must_be_ordered_spoken_and_inside_the_video(
    cues: list[dict[str, Any]],
) -> None:
    with pytest.raises(RecipeError):
        build_recipe(minimal(narration=cues))


def test_a_missing_file_and_bad_json_both_raise_recipe_errors(tmp_path: Path) -> None:
    with pytest.raises(RecipeError, match="cannot read"):
        load_recipe(tmp_path / "nope.json")
    broken = tmp_path / "broken.json"
    broken.write_text("{not json", encoding="utf-8")
    with pytest.raises(RecipeError, match="not valid JSON"):
        load_recipe(broken)


def test_loading_is_a_pure_function_of_the_file() -> None:
    first, second = load_recipe(FIXTURE), load_recipe(FIXTURE)
    assert isinstance(first, Recipe) and first == second
