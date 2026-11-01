"""The gallery: one patch and one picture for every built-in source and shaper, and the page that shows them.

An architecture test of the tooling and of the extension point: its subject is every built-in source and shaper, so a
new one is covered the moment it is registered and fails here until it has its patch (`poe new-source` writes one).
No picture is compared: what a picture shows is checked by tests/test_plotting.py, and the patches by their numbers.
"""

import importlib.util
import shutil
from pathlib import Path

import numpy as np
import pytest

from bullterrier_control import UnknownNameError, load_patch, registry

ROOT = Path(__file__).resolve().parents[2]


def tool():
    spec = importlib.util.spec_from_file_location("gallery", ROOT / "tools" / "gallery.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gallery = tool()
NAMES = sorted(
    name
    for name, thing in registry.all_registered().items()
    # one the Rust core runs has no Python function; the others are defined in the package's own modules
    if thing.kind != "operation"
    and (thing.function is None or thing.function.__module__.startswith("bullterrier_control.builtin."))
)
PATCHES = ROOT / "examples" / "gallery"


def test_the_gallery_has_a_patch_for_every_built_in_source_and_shaper_and_no_other():
    assert sorted(path.stem for path in PATCHES.glob("*.toml")) == NAMES


@pytest.mark.parametrize("name", NAMES)
def test_each_patch_builds_on_its_own_and_its_node_uses_the_thing_it_is_named_for(name):
    patch = load_patch(PATCHES / f"{name}.toml")
    assert patch.nodes[name].use == name
    assert patch.nodes[name].returns == "signal"
    built = patch.build()  # no bundle: a gallery patch holds its own data
    times = np.linspace(0, 4, 401)  # the seconds each picture draws
    for node, signal in built.items():
        assert np.isfinite(signal.sample(times)).all(), node


@pytest.mark.parametrize("name", [name for name in NAMES if name != "constant"])
def test_each_picture_shows_something_moving_not_a_flat_line(name):
    signal = load_patch(PATCHES / f"{name}.toml").build()[name]
    values = signal.sample(np.linspace(0, 4, 401))
    assert values.max() - values.min() > 0.1


@pytest.mark.parametrize("name", NAMES)
def test_each_patch_has_its_committed_picture(name):
    assert (ROOT / "docs" / "gallery" / f"{name}.png").read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"


def test_the_committed_page_is_the_one_the_patches_and_summaries_make():
    # pictures are not compared (they depend on the drawing library's version); the text that frames them is
    assert (ROOT / "docs" / "gallery.md").read_text(encoding="utf-8") == gallery.page(ROOT)


@pytest.fixture
def small_checkout(tmp_path):
    (tmp_path / "examples" / "gallery").mkdir(parents=True)
    for name in ("clip", "envelope"):
        shutil.copy(PATCHES / f"{name}.toml", tmp_path / "examples" / "gallery")
    return tmp_path


def test_writing_the_gallery_draws_one_picture_per_patch_and_the_page_listing_them(small_checkout):
    written = gallery.write_gallery(small_checkout)
    pictures = [small_checkout / "docs" / "gallery" / f"{name}.png" for name in ("clip", "envelope")]
    assert sorted(written) == sorted([*pictures, small_checkout / "docs" / "gallery.md"])
    for picture in pictures:
        assert picture.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"
    page = (small_checkout / "docs" / "gallery.md").read_text(encoding="utf-8")
    assert "![envelope](gallery/envelope.png)" in page
    assert "![clip](gallery/clip.png)" in page
    assert 'use = "envelope"' in page  # the patch itself is shown
    assert registry.get("envelope").summary in page


def test_a_node_that_gives_events_feeds_a_picture_but_is_not_drawn(small_checkout):
    (small_checkout / "examples" / "gallery" / "envelope.toml").write_text(
        'format = "bullterrier-control/patch"\nversion = 1\n'
        '[strong]\nuse = "select"\nevents = [{ t = 1, strength = 0.9 }, { t = 2, strength = 0.2 }]\nthreshold = 0.5\n'
        '[envelope]\nuse = "envelope"\nevents = "strong"\n',
        encoding="utf-8",
    )
    written = gallery.write_gallery(small_checkout)
    assert small_checkout / "docs" / "gallery" / "envelope.png" in written


def test_the_page_lists_sources_before_shapers_and_each_by_name(small_checkout):
    page = gallery.page(small_checkout)
    assert page.index("### `envelope`") < page.index("### `clip`")  # a source, then a shaper
    assert page.index("## Sources") < page.index("### `envelope`") < page.index("## Shapers") < page.index("### `clip`")


def test_a_patch_for_a_name_that_is_not_registered_is_refused_not_listed(small_checkout):
    (small_checkout / "examples" / "gallery" / "nosuch.toml").write_text("x", encoding="utf-8")
    with pytest.raises(UnknownNameError, match="nosuch"):
        gallery.page(small_checkout)
