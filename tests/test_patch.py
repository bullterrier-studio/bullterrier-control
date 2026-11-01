"""A patch: a named graph of sources, shapers and operations with their values, as data.

Every expected number is worked out by hand (docs/examples.md, sections 4 and 7, and the exponentials of an envelope).
"""

import copy
import json
import subprocess
import sys
import tomllib

import jsonschema
import numpy as np
import pytest

from bullterrier_control import (
    Bundle,
    FormatError,
    ParameterError,
    Patch,
    Signal,
    UnknownNameError,
    from_values,
    lib,
    load_patch,
    registry,
)
from bullterrier_control.patch import patch_schema
from bullterrier_control.plugin import Constant, Finite, Params, param, source

HEADER = {"format": "bullterrier-control/patch", "version": 1}
KICKS = [
    {"t": 0.5, "strength": 0.9},
    {"t": 0.7, "strength": 0.45},
    {"t": 1.0, "strength": 0.8},
    {"t": 1.05, "strength": 0.95},
    {"t": 2.0, "strength": 0.7},
]
SECTION_BUNDLE = {
    "format": "bullterrier-control/bundle",
    "version": 1,
    "events": {
        "kick": [{"t": 0.5, "strength": 1.0}, {"t": 1.0, "strength": 0.6}, {"t": 1.5, "strength": 1.0}],
    },
    "series": {"bass": {"rate": 10, "values": [0.0, 1.0], "unit": "ratio"}},
    "grid": {"beats": [0.0, 0.5, 1.1, 1.8], "meter": 4, "first_downbeat": 0},
}


def patch_of(**nodes: object) -> dict[str, object]:
    return {**HEADER, **nodes}


# ---------------------------------------------------------------- building: the numbers, by hand


def test_example_seven_as_a_patch_gives_the_numbers_of_the_worked_example():
    patch = Patch.parse(
        patch_of(
            strong={"use": "select", "events": KICKS, "threshold": 0.5},
            spaced={"use": "thin", "events": "strong", "gap": 0.25},
            flash={"use": "envelope", "events": "spaced", "decay": 0.25},
        )
    )
    built = patch.build()
    assert list(built) == ["strong", "spaced", "flash"]  # every node, in file order
    assert [event.t for event in built["spaced"]] == [0.5, 1.0, 2.0]
    # 0.9 at the first kick; 0.9 * e^-1; the second kick 0.8; 0.8 * e^-0.4 (not the thinned 0.95 kick's); the last kick
    values = built["flash"].sample([0.5, 0.75, 1.0, 1.1, 2.0])
    assert values.tolist() == pytest.approx([0.9, 0.33109149705429813, 0.8, 0.5362560368285115, 0.7], abs=1e-9)


def test_the_events_of_a_bundle_are_named_by_their_path_in_it():
    bundle = Bundle.parse(SECTION_BUNDLE)
    patch = Patch.parse(
        patch_of(
            strong={"use": "select", "events": "events.kick", "threshold": 0.7},
            glow={"use": "envelope", "events": "strong", "decay": 0.25},
        )
    )
    glow = patch.build(bundle)["glow"]
    # the 0.6 kick at 1.0 s is below the threshold: at 1.0 only the first kick's e^-2 is left
    values = glow.sample([0.5, 0.75, 1.0, 1.5, 1.75])
    assert values.tolist() == pytest.approx(
        [1.0, 0.36787944117144233, 0.1353352832366127, 1.0, 0.36787944117144233], abs=1e-9
    )


def test_a_series_of_a_bundle_is_read_linearly_where_a_signal_is_wanted():
    bundle = Bundle.parse(SECTION_BUNDLE)
    patch = Patch.parse(patch_of(limited={"use": "clip", "signal": "series.bass", "low": 0.1, "high": 0.6}))
    # 0 at 0 s and 1 at 0.1 s: halfway at 0.05 s (a held series would read 0 there and clip up to 0.1); held after
    values = patch.build(bundle)["limited"].sample([0.0, 0.05, 0.08, 1.0])
    assert values.tolist() == pytest.approx([0.1, 0.5, 0.6, 0.6], abs=1e-9)


def test_a_node_that_gives_a_series_is_read_linearly_too():
    ramp = {"rate": 10, "values": [float(value) for value in range(11)], "unit": "ratio"}
    patch = Patch.parse(
        patch_of(
            scaled={"use": "normalise", "series": ramp, "lower": 0.1, "upper": 0.9},
            followed={"use": "clip", "signal": "scaled", "low": 0.0, "high": 1.0},
        )
    )
    # the percentiles of 0..10 are 1 and 9: sample 2 is (2 - 1) / 8, and 0.25 s is halfway between samples 2 and 3
    values = patch.build()["followed"].sample([0.2, 0.25])
    assert values.tolist() == pytest.approx([0.125, 0.1875], abs=1e-9)
    assert patch.nodes["scaled"].returns == "series"


def test_the_grid_of_a_bundle_is_named_grid():
    bundle = Bundle.parse(SECTION_BUNDLE)
    patch = Patch.parse(patch_of(pulse={"use": "lfo", "per": "beat", "grid": "grid"}))
    # beats at 0, 0.5, 1.1, 1.8: a quarter of the first beat is 0.125 s (the high of a sine), three quarters 0.375 s
    values = patch.build(bundle)["pulse"].sample([0.0, 0.125, 0.375, 0.5])
    assert values.tolist() == pytest.approx([0.5, 1.0, 0.0, 0.5], abs=1e-9)


def test_an_inline_grid_and_a_toml_integer_for_a_decimal_are_accepted():
    patch = Patch.parse(
        patch_of(
            pulse={"use": "lfo", "per": "beat", "rate": 1, "grid": SECTION_BUNDLE["grid"]},
            level={"use": "constant", "value": 1},
        )
    )
    built = patch.build()
    assert built["pulse"].sample(0.125) == pytest.approx(1.0, abs=1e-9)
    assert built["level"].sample(3.0) == 1.0


def test_each_node_says_what_it_gives_without_building_anything():
    patch = Patch.parse(
        patch_of(
            strong={"use": "select", "events": "events.kick"},
            glow={"use": "envelope", "events": "strong"},
            slow={"use": "slew", "signal": "glow", "rise": 1},
            ramp={"use": "normalise", "series": "series.bass"},
        )
    )
    assert {name: node.returns for name, node in patch.nodes.items()} == {
        "strong": "events",
        "glow": "signal",
        "slow": "signal",
        "ramp": "series",
    }
    assert patch.nodes["slow"].use == "slew"


def test_a_patch_given_as_a_mapping_is_read_without_touching_it():
    # a video project holds its patches as tables of its own file: `parse` is the way in for those
    data = patch_of(glow={"use": "envelope", "events": KICKS, "decay": 0.25})
    before = copy.deepcopy(data)
    Patch.parse(data).build()
    assert data == before


def test_building_twice_gives_the_same_values():
    patch = Patch.parse(patch_of(glow={"use": "envelope", "events": KICKS, "decay": 0.25}))
    times = [0.4, 0.5, 0.9, 3.0]
    assert patch.build()["glow"].sample(times).tolist() == patch.build()["glow"].sample(times).tolist()


# ---------------------------------------------------------------- building only some nodes


NEEDS_A_BUNDLE_AND_NOT = patch_of(
    strong={"use": "select", "events": "events.kick"},
    glow={"use": "envelope", "events": "strong"},
    wave={"use": "lfo"},
)


def test_building_some_nodes_builds_only_those_so_the_rest_need_no_bundle():
    built = Patch.parse(NEEDS_A_BUNDLE_AND_NOT).build(only=["wave"])
    assert list(built) == ["wave"]
    assert built["wave"].sample(0.25) == pytest.approx(1.0, abs=1e-9)


def test_building_a_node_builds_what_it_uses_but_gives_back_only_what_was_asked_for():
    patch = Patch.parse(NEEDS_A_BUNDLE_AND_NOT)
    built = patch.build(Bundle.parse(SECTION_BUNDLE), only=["glow"])
    assert list(built) == ["glow"]
    with pytest.raises(FormatError, match="bundle"):  # what it uses does need the bundle
        patch.build(only=["glow"])


def test_the_nodes_asked_for_come_back_in_file_order():
    built = Patch.parse(NEEDS_A_BUNDLE_AND_NOT).build(Bundle.parse(SECTION_BUNDLE), only=["wave", "strong"])
    assert list(built) == ["strong", "wave"]


def test_asking_for_a_node_the_patch_lacks_names_it_and_what_the_patch_has():
    with pytest.raises(UnknownNameError, match="nosuch") as refused:
        Patch.parse(NEEDS_A_BUNDLE_AND_NOT).build(only=["nosuch"])
    assert "wave" in str(refused.value)


# ---------------------------------------------------------------- building: what the bundle must hold


def test_a_path_into_a_bundle_needs_a_bundle_and_says_which_node_and_key():
    patch = Patch.parse(patch_of(glow={"use": "envelope", "events": "events.kick"}))
    with pytest.raises(FormatError, match="bundle") as refused:
        patch.build()
    assert str(refused.value).startswith("glow.events: ")


BUNDLE_LACKS = {
    "a name the bundle has no events for": ({"use": "envelope", "events": "events.snare"}, "glow.events", "kick"),
    "a name the bundle has no series for": ({"use": "clip", "signal": "series.treble"}, "glow.signal", "bass"),
}


@pytest.mark.parametrize(("node", "path", "has"), BUNDLE_LACKS.values(), ids=BUNDLE_LACKS.keys())
def test_a_name_the_bundle_lacks_is_refused_at_the_key_naming_what_it_has(node, path, has, reports):
    patch = Patch.parse(patch_of(glow=node))
    with pytest.raises(FormatError) as refused:
        patch.build(Bundle.parse(SECTION_BUNDLE))
    reports(refused.value, path)
    assert has in str(refused.value)


def test_a_bundle_without_a_grid_is_refused_where_a_grid_is_named():
    patch = Patch.parse(patch_of(pulse={"use": "lfo", "per": "beat", "grid": "grid"}))
    without = {key: value for key, value in SECTION_BUNDLE.items() if key != "grid"}
    with pytest.raises(FormatError, match="grid") as refused:
        patch.build(Bundle.parse(without))
    assert str(refused.value).startswith("pulse.grid: ")


def test_an_error_a_thing_raises_when_built_keeps_its_kind_and_names_the_node(throwaway_registry):
    class Nothing(Params):
        """No parameters."""

    @source(params=Nothing)
    def broken(params: Nothing) -> Signal:
        """A source that cannot be made."""
        raise ParameterError("it cannot be made", (("depth", "is too deep"),))

    with pytest.raises(ParameterError, match="it cannot be made") as refused:
        Patch.parse(patch_of(low={"use": "broken"})).build()
    assert str(refused.value).startswith("low: ")
    assert refused.value.problems == (("low.depth", "is too deep"),)


# ---------------------------------------------------------------- parsing: every rule, with the path it names

LATER = {
    "a": {"use": "envelope", "events": "b"},
    "b": {"use": "select", "events": "events.kick"},
}
LEVEL = {"use": "constant"}
STRONG = {"use": "select", "events": "events.kick"}

BAD = {
    "an unknown source": (patch_of(a={"use": "nosuch"}), "a.use"),
    "a use that is not text": (patch_of(a={"use": 3}), "a.use"),
    "no use": (patch_of(a={"value": 1}), "a.use"),
    "a node that is not a table": (patch_of(glow=5), "glow"),
    "a signal given to a source": (patch_of(level=LEVEL, a={"use": "constant", "signal": "level"}), "a.signal"),
    "a shaper without its signal": (patch_of(a={"use": "slew"}), "a.signal"),
    "a reference to a later node": (patch_of(**LATER), "a.events"),
    "a reference to the node itself": (patch_of(a={"use": "envelope", "events": "a"}), "a.events"),
    "a reference to a node that does not exist": (patch_of(a={"use": "envelope", "events": "nothing"}), "a.events"),
    "a node where events are wanted that gives a signal": (
        patch_of(level=LEVEL, a={"use": "envelope", "events": "level"}),
        "a.events",
    ),
    "a node where a signal is wanted that gives events": (
        patch_of(strong=STRONG, a={"use": "slew", "signal": "strong"}),
        "a.signal",
    ),
    "bundle events where a grid is wanted": (
        patch_of(a={"use": "lfo", "per": "beat", "grid": "events.kick"}),
        "a.grid",
    ),
    "a bundle series where a grid is wanted": (
        patch_of(a={"use": "lfo", "per": "beat", "grid": "series.bass"}),
        "a.grid",
    ),
    "a bundle grid where events are wanted": (patch_of(a={"use": "envelope", "events": "grid"}), "a.events"),
    "a bundle series where events are wanted": (patch_of(a={"use": "envelope", "events": "series.bass"}), "a.events"),
    "a reference where only data goes (keyframes)": (
        patch_of(a={"use": "keyframes", "points": "events.kick"}),
        "a.points",
    ),
    "a reference where only data goes (values)": (patch_of(a={"use": "steps", "values": "kicks"}), "a.values"),
    "a bundle name that no bundle could hold": (patch_of(a={"use": "envelope", "events": "events.Kick"}), "a.events"),
    "a bundle path with no name": (patch_of(a={"use": "envelope", "events": "events."}), "a.events"),
    "a name after grid": (patch_of(a={"use": "lfo", "per": "beat", "grid": "grid.x"}), "a.grid"),
    "an input the thing does not take": (patch_of(a={"use": "constant", "events": "events.kick"}), "a.events"),
    "a required input left out": (patch_of(a={"use": "envelope"}), "a.events"),
    "a key that is neither input nor parameter": (
        patch_of(glow={"use": "envelope", "events": KICKS, "target": "radius"}),
        "glow.target",
    ),
    "a parameter beyond its range": (patch_of(glow={"use": "envelope", "events": KICKS, "decay": -1}), "glow.decay"),
    "a bool for a number": (patch_of(glow={"use": "envelope", "events": KICKS, "decay": True}), "glow.decay"),
    "a text for a number": (patch_of(glow={"use": "envelope", "events": KICKS, "decay": "0.25"}), "glow.decay"),
    "a choice not offered": (patch_of(glow={"use": "envelope", "events": KICKS, "curve": "wobbly"}), "glow.curve"),
    "inline events with a bad strength": (
        patch_of(glow={"use": "envelope", "events": [{"t": 0}, {"t": 1, "strength": 5}]}),
        "glow.events[1].strength",
    ),
    "inline keyframes that do not increase": (
        patch_of(a={"use": "keyframes", "points": [{"t": 1, "value": 0}, {"t": 1, "value": 1}]}),
        "a.points[1].t",
    ),
    "a node named grid": (patch_of(grid=LEVEL), "grid"),
    "a node named events": (patch_of(events=LEVEL), "events"),
    "a node named series": (patch_of(series=LEVEL), "series"),
    "a node name in capitals": (patch_of(Glow=LEVEL), "Glow"),
    "a node name starting with a digit": (patch_of(**{"1st": LEVEL}), "1st"),
    "a top-level value that is not a node": (patch_of(drives="glow_radius", level=LEVEL), "drives"),
    "no format": ({"version": 1, "level": LEVEL}, "format"),
    "another format": ({**HEADER, "format": "bullterrier-control/bundle"}, "format"),
    "no version": ({"format": HEADER["format"], "level": LEVEL}, "version"),
    "a version this reader does not know": ({**HEADER, "version": 2}, "version"),
    "a version written as a decimal": ({**HEADER, "version": 1.0}, "version"),
    "a version that is true": ({**HEADER, "version": True}, "version"),
}


@pytest.mark.parametrize(("data", "path"), BAD.values(), ids=BAD.keys())
def test_a_broken_patch_is_one_format_error_naming_the_node_and_key(data, path, reports):
    with pytest.raises(FormatError) as refused:
        Patch.parse(data)
    reports(refused.value, path)


def test_every_problem_is_listed_not_only_the_first(reports):
    data = patch_of(
        a={"use": "nosuch"},
        b={"use": "envelope", "events": "events.Kick", "decay": -1},
        Loud=LEVEL,
    )
    with pytest.raises(FormatError) as refused:
        Patch.parse(data)
    for path in ("a.use", "b.events", "b.decay", "Loud"):
        reports(refused.value, path)
    assert {path for path, _ in refused.value.problems} >= {"a.use", "b.events", "b.decay", "Loud"}


def test_an_unknown_source_lists_what_is_available():
    with pytest.raises(FormatError) as refused:
        Patch.parse(patch_of(a={"use": "envlope"}))
    assert "envelope" in str(refused.value)


FOLLOWERS = {
    "a shaper taking a signal": {"use": "slew", "signal": "a"},
    "an envelope taking events": {"use": "envelope", "events": "a"},
    "a normalise taking a series": {"use": "normalise", "series": "a"},
}


@pytest.mark.parametrize("follower", FOLLOWERS.values(), ids=FOLLOWERS.keys())
def test_a_node_that_follows_a_node_with_a_broken_use_adds_no_second_complaint(follower):
    # what the broken node would have given is unknown, so no reference to it can be judged
    with pytest.raises(FormatError) as refused:
        Patch.parse(patch_of(a={"use": "nosuch"}, b=follower))
    assert [path for path, _ in refused.value.problems] == ["a.use"]


@pytest.mark.parametrize("key", [1, None, (1, 2)], ids=["an int", "none", "a tuple"])
def test_a_key_that_is_not_text_is_a_format_error_naming_it_not_a_type_error(key):
    # a patch held in another file (a YAML project, say) can have keys of any type
    with pytest.raises(FormatError) as refused:
        Patch.parse({**HEADER, key: LEVEL})
    assert str(key) in [path for path, _ in refused.value.problems]


def test_a_patch_with_no_node_is_valid_and_builds_nothing():
    assert Patch.parse(HEADER).build() == {}


def test_a_rule_that_ties_parameters_to_inputs_is_the_things_own_and_is_reported_when_it_is_built():
    # `per = "beat"` needs a grid: the lfo says so when called, so a patch alone cannot be refused for it at read time
    patch = Patch.parse(patch_of(a={"use": "lfo", "per": "beat"}))
    with pytest.raises(ParameterError, match="grid") as refused:
        patch.build()
    assert str(refused.value).startswith("a: ")


def test_the_keys_of_a_patch_are_signals_only_never_a_target():
    # mapping a signal onto a parameter of something else is a consumer's job (video's): a patch cannot say it
    for key in ("target", "drives", "maps_to", "range"):
        with pytest.raises(FormatError):
            Patch.parse(patch_of(glow={"use": "envelope", "events": KICKS, key: "glow_radius"}))


# ---------------------------------------------------------------- files


GLOW = """\
format = "bullterrier-control/patch"
version = 1

[strong]
use = "select"
events = "events.kick"   # the bundle's events "kick"
threshold = 0.7

[glow]
use = "envelope"
events = "strong"        # the node above
decay = 0.25
"""


def test_a_toml_file_is_loaded_and_builds_against_a_bundle(tmp_path):
    path = tmp_path / "glow.toml"
    path.write_text(GLOW, encoding="utf-8")
    values = load_patch(path).build(Bundle.parse(SECTION_BUNDLE))["glow"].sample([0.5, 0.75, 1.0])
    assert values.tolist() == pytest.approx([1.0, 0.36787944117144233, 0.1353352832366127], abs=1e-9)


def test_a_toml_file_and_the_same_table_in_python_are_one_patch(tmp_path):
    path = tmp_path / "glow.toml"
    path.write_text(GLOW, encoding="utf-8")
    assert load_patch(path) == Patch.parse(tomllib.loads(GLOW))


FILES = {
    "toml that is not toml": ("format = \n", "not TOML"),
    "a repeated table": (GLOW + '\n[glow]\nuse = "constant"\n', "not TOML"),
    "a patch that breaks a rule": (GLOW.replace("0.25", "-1"), "glow.decay"),
}


@pytest.mark.parametrize(("text", "named"), FILES.values(), ids=FILES.keys())
def test_a_broken_file_is_a_format_error_that_starts_with_its_path(text, named, tmp_path):
    path = tmp_path / "broken.toml"
    path.write_text(text, encoding="utf-8")
    with pytest.raises(FormatError, match=named) as refused:
        load_patch(path)
    assert str(refused.value).startswith(str(path))


def test_a_file_that_is_not_utf8_is_refused_naming_the_file(tmp_path):
    path = tmp_path / "latin.toml"
    path.write_bytes(b'format = "caf\xe9"\n')
    with pytest.raises(FormatError, match="UTF-8") as refused:
        load_patch(path)
    assert str(refused.value).startswith(str(path))


PARSER_LIMITS = {
    "arrays nested deeper than the parser can follow": "x = " + "[" * 5000 + "]" * 5000 + "\n",
    "a whole number of thousands of digits": '[a]\nuse = "constant"\nvalue = 1' + "0" * 5000 + "\n",
}


@pytest.mark.parametrize("text", PARSER_LIMITS.values(), ids=PARSER_LIMITS.keys())
def test_a_file_the_toml_parser_cannot_follow_is_a_format_error_naming_the_file(text, tmp_path):
    path = tmp_path / "deep.toml"
    path.write_text(text, encoding="utf-8")
    with pytest.raises(FormatError) as refused:
        load_patch(path)
    assert str(refused.value).startswith(str(path))


def test_a_file_that_is_missing_is_the_error_the_system_gave(tmp_path):
    with pytest.raises(FileNotFoundError):
        load_patch(tmp_path / "absent.toml")


# ---------------------------------------------------------------- saving: a graph made in code, as a patch


# the chain below, by the rules of a save: each unnamed thing upstream is a node `<name>_<n>` above the one it feeds,
# every parameter written with its value (a default too: slew's and follow's `rate` is 1000), floats as Python writes
# them
LEVEL_CHAIN = """\
format = "bullterrier-control/patch"
version = 1

[level_1]
use = "constant"
value = 0.25

[level_2]
use = "slew"
signal = "level_1"
rise = 0.5
fall = 2.0
rate = 1000.0

[level]
use = "follow"
signal = "level_2"
attack = 0.1
release = 0.3
rate = 1000.0
"""
TIMES = [-1.0, 0.0, 0.001, 0.25, 0.5, 1.0, 7.5, 1e4]


def level_chain() -> Signal:
    return lib.follow(lib.slew(lib.constant(value=0.25), rise=0.5, fall=2.0), attack=0.1, release=0.3)


def test_a_chain_made_through_lib_is_saved_as_the_hand_written_patch():
    assert Patch.from_signals({"level": level_chain()}).to_toml() == LEVEL_CHAIN


def test_a_saved_file_loads_back_as_an_equal_patch_that_builds_the_same_samples(tmp_path):
    level = level_chain()
    saved = Patch.from_signals({"level": level})
    path = tmp_path / "level.toml"
    saved.save(path)
    assert path.read_text(encoding="utf-8") == LEVEL_CHAIN
    loaded = load_patch(path)
    assert loaded == saved
    assert loaded.build()["level"].sample(TIMES).tolist() == level.sample(TIMES).tolist()


def test_signals_are_saved_in_the_order_given_each_after_what_it_is_made_of():
    saved = Patch.from_signals({"slow": lib.slew(lib.constant(), rise=1.0), "level": lib.constant(value=1.0)})
    assert list(saved.nodes) == ["slow_1", "slow", "level"]


def test_a_patch_read_first_thing_in_a_process_finds_the_installed_packages():
    """In a fresh process: reading a patch is a query of the registry, so it loads the packages (lfo's is Python)."""
    code = (
        "from bullterrier_control import Patch\n"
        "patch = Patch.parse({'format': 'bullterrier-control/patch', 'version': 1, 'wave': {'use': 'lfo'}})\n"
        "print(patch.nodes['wave'].use)\n"
    )
    done = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, encoding="utf-8", check=False)
    assert (done.returncode, done.stdout.strip()) == (0, "lfo"), done.stderr


def test_a_built_patch_saves_as_what_it_was_read_from():
    bundle = Bundle.parse(SECTION_BUNDLE)
    built = Patch.parse(tomllib.loads(GLOW)).build(bundle)
    again = Patch.from_signals({"glow": built["glow"]})
    # the bundle's events were selected when built: the save holds them inline, so it needs no bundle
    assert list(again.nodes) == ["glow"]
    assert again.build()["glow"].sample(TIMES).tolist() == built["glow"].sample(TIMES).tolist()


class Height(Params):
    """A height."""

    value: Finite = param(0.5, doc="The height, as a share of the range.", unit="fraction", min=0, max=1)


def test_a_signal_of_a_python_plugin_is_saved_and_read_back_in_the_same_registry(throwaway_registry):
    @source(params=Height)
    def height(params: Height) -> Signal:
        """A height that holds."""
        return Constant(params.value)

    raised = lib.slew(height(value=0.75), rise=0.5)
    text = Patch.from_signals({"raised": raised}).to_toml()
    assert '[raised_1]\nuse = "height"\nvalue = 0.75\n' in text
    rebuilt = Patch.parse(tomllib.loads(text)).build()["raised"]
    # from 0.75 at time 0, slewed from where it starts: the same signal
    assert rebuilt.sample(TIMES).tolist() == raised.sample(TIMES).tolist() == [0.75] * len(TIMES)


class Doubled(Signal):
    """A signal a Python `compute` makes."""

    def compute(self, times):
        return np.asarray(times) * 2.0


UNSAVED = {
    "a Constant of Python": ({"level": Constant(0.5)}, "level", "values given as they are"),
    "values given": ({"level": from_values([0.0, 1.0], rate=2.0)}, "level", "values given as they are"),
    "a signal of its own compute": ({"level": Doubled()}, "level", "callback"),
    "a sum": ({"mix": lib.constant(value=0.5) + lib.constant(value=0.25)}, "mix", "operator"),
    "a shaper over a sum": ({"glow": lib.slew(lib.constant() * 2.0)}, "glow.signal", "operator"),
    "a name no node may have": ({"Glow": lib.constant()}, "Glow", "node name"),
}


@pytest.mark.parametrize(("signals", "path", "reason"), UNSAVED.values(), ids=UNSAVED.keys())
def test_a_signal_a_patch_cannot_hold_is_refused_naming_its_path_and_why(signals, path, reason):
    with pytest.raises(FormatError, match="cannot save as a patch") as refused:
        Patch.from_signals(signals)
    assert [where for where, _ in refused.value.problems] == [path]
    assert reason in refused.value.problems[0][1]


# ---------------------------------------------------------------- the schema


def test_the_schema_accepts_every_good_patch_and_refuses_one_that_is_misshapen():
    schema = patch_schema()
    jsonschema.Draft202012Validator.check_schema(schema)
    for good in (HEADER, tomllib.loads(GLOW), patch_of(a={"use": "envelope", "events": KICKS, "decay": 0.25})):
        jsonschema.validate(json.loads(json.dumps(good)), schema)
    for misshapen in {
        "another version": {**HEADER, "version": 2},
        "another format": {**HEADER, "format": "bullterrier-control/bundle"},
        "no header": {"glow": {"use": "constant"}},
        "a node without use": patch_of(glow={"value": 1}),
        "a node that is not a table": patch_of(glow=5),
        "a node name in capitals": patch_of(Glow={"use": "constant"}),
        "a node named grid": patch_of(grid={"use": "constant"}),
        "a use that is not text": patch_of(glow={"use": 3}),
    }.values():
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate(misshapen, schema)


def test_the_schema_agrees_with_the_reader_on_every_structural_row_of_the_bad_table():
    schema = patch_schema()
    structural = [
        "a use that is not text",
        "no use",
        "a node that is not a table",
        "a node named grid",
        "a node name in capitals",
        "a top-level value that is not a node",
        "no format",
        "another format",
        "a version this reader does not know",
    ]
    validator = jsonschema.Draft202012Validator(schema)
    for name in structural:
        assert not validator.is_valid(json.loads(json.dumps(BAD[name][0]))), name


@pytest.mark.parametrize("name", sorted(registry.all_registered()))
def test_every_registered_name_is_a_valid_use_in_the_schema(name):
    # the schema and the registry spell the rule for a name; a registered name the schema refused could not be used
    jsonschema.validate(patch_of(a={"use": name}), patch_schema())
