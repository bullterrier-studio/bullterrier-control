"""Rebuild docs/images/banner.svg, the README's header: six signals, each drawn from the values the library returns.

    uv run python tools/make_banner.py

Every curve is `signal.sample(times)` of the call printed under it; the dot is the value read at one moment,
printed above it.
"""

from pathlib import Path

import numpy as np

from bullterrier_control import Signal, lib

ROOT = Path(__file__).resolve().parent.parent
WIDTH, HEIGHT, MARGIN, GAP, TILE_HEIGHT, PAD = 1632, 380, 36, 18, 230, 26
BACKGROUND, TILE, GRID, TEXT, MUTED, ACCENT = "#0d0e12", "#16181e", "#2a2d36", "#e8eaf0", "#828794", "#ff7a2f"
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
READ_AT = 0.62  # where in each tile's time span the dot reads the signal
SAMPLES = 480

KICKS = [
    {"t": 0.35, "strength": 1.0},
    {"t": 1.45, "strength": 0.65},
    {"t": 2.2, "strength": 0.9},
    {"t": 3.2, "strength": 0.7},
]
GATE = lib.lfo(shape="square", rate=0.5, duty=0.4)

# (name, the call shown under the tile, the signal, seconds drawn, the unsmoothed input to show behind it, if any)
TILES: list[tuple[str, str, Signal, float, Signal | None]] = [
    (
        "keyframes",
        "lib.keyframes(points=[...])",
        lib.keyframes(
            points=[
                {"t": 0, "value": 0.1, "ease": "in-out"},
                {"t": 2, "value": 0.9, "ease": "out"},
                {"t": 4, "value": 0.25, "ease": "in-out"},
                {"t": 6, "value": 0.75},
            ]
        ),
        6.0,
        None,
    ),
    ("lfo", "lib.lfo(rate=0.75)", lib.lfo(rate=0.75, phase=0.1), 4.0, None),
    ("envelope", "lib.envelope(events, decay=0.35)", lib.envelope(events=KICKS, attack=0.02, decay=0.35), 4.0, None),
    (
        "steps",
        "lib.steps(values=[...], rate=2)",
        lib.steps(values=[0.2, 0.9, 0.5, 1.0, 0.3, 0.7], rate=2),
        3.0,
        None,
    ),
    ("noise", "lib.noise(seed=7, rate=2)", lib.noise(seed=7, rate=2, shape="smooth"), 4.0, None),
    ("slew", "lib.slew(gate, rise=0.3, fall=0.7)", lib.slew(GATE, rise=0.3, fall=0.7), 4.0, GATE),
]


def path(signal: Signal, seconds: float, left: float, top: float, width: float, height: float) -> str:
    """The signal over `seconds` as an SVG path in a box; y runs from `top` (value 1) to `top + height` (value 0)."""
    times = np.linspace(0.0, seconds, SAMPLES)
    values = np.clip(signal.sample(times), 0.0, 1.0)
    points = [(left + width * t / seconds, top + height * (1.0 - v)) for t, v in zip(times, values, strict=True)]
    return "".join(f"{'ML'[i > 0]}{x:.1f} {y:.1f}" for i, (x, y) in enumerate(points))


def tile(index: int, name: str, call: str, signal: Signal, seconds: float, ghost: Signal | None) -> str:
    """One tile: grid, the curve with a faint area under it, the dot, and the name and call underneath."""
    tile_width = (WIDTH - 2 * MARGIN - GAP * (len(TILES) - 1)) / len(TILES)
    x = MARGIN + index * (tile_width + GAP)
    left, top, width, height = x + PAD, MARGIN + PAD, tile_width - 2 * PAD, TILE_HEIGHT - 2 * PAD
    bottom = top + height
    curve = path(signal, seconds, left, top, width, height)
    moment = seconds * READ_AT
    value = float(np.clip(signal.sample(moment), 0.0, 1.0))
    dot_x, dot_y = left + width * READ_AT, top + height * (1.0 - value)
    parts = [f'<rect x="{x:.1f}" y="{MARGIN}" width="{tile_width:.1f}" height="{TILE_HEIGHT}" rx="10" fill="{TILE}"/>']
    parts += [
        f'<line x1="{left:.1f}" y1="{y:.1f}" x2="{left + width:.1f}" y2="{y:.1f}" stroke="{GRID}" stroke-width="1"/>'
        for y in (top, top + height / 2, bottom)
    ]
    if ghost is not None:
        parts.append(
            f'<path d="{path(ghost, seconds, left, top, width, height)}" fill="none" stroke="{MUTED}" '
            'stroke-width="2" stroke-dasharray="5 6" stroke-linejoin="round"/>'
        )
    parts += [
        f'<path d="{curve}L{left + width:.1f} {bottom:.1f}L{left:.1f} {bottom:.1f}Z" fill="{ACCENT}" opacity="0.12"/>',
        f'<path d="{curve}" fill="none" stroke="{ACCENT}" stroke-width="3.5" '
        'stroke-linecap="round" stroke-linejoin="round"/>',
        f'<line x1="{dot_x:.1f}" y1="{top}" x2="{dot_x:.1f}" y2="{bottom:.1f}" stroke="{MUTED}" '
        'stroke-width="1" stroke-dasharray="3 4"/>',
        f'<circle cx="{dot_x:.1f}" cy="{dot_y:.1f}" r="6" fill="{ACCENT}" stroke="{TILE}" stroke-width="3"/>',
        f'<text x="{dot_x:.1f}" y="{MARGIN + 17}" fill="{TEXT}" font-size="13" text-anchor="middle">{value:.2f}</text>',
        f'<text x="{x:.1f}" y="{MARGIN + TILE_HEIGHT + 34}" fill="{TEXT}" font-size="22">{name}</text>',
        f'<text x="{x:.1f}" y="{MARGIN + TILE_HEIGHT + 58}" fill="{MUTED}" font-size="12">{call}</text>',
    ]
    return "\n".join(parts)


def main() -> None:
    """Write docs/images/banner.svg and print its path."""
    tiles = "\n".join(tile(i, *spec) for i, spec in enumerate(TILES))
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}" role="img" '
        'aria-label="Six control signals drawn by bullterrier-control: keyframes, lfo, envelope, steps, noise, slew">\n'
        f'<rect width="{WIDTH}" height="{HEIGHT}" fill="{BACKGROUND}"/>\n'
        f'<g font-family="{MONO}">\n{tiles}\n'
        f'<text x="{WIDTH - MARGIN}" y="{HEIGHT - 28}" fill="{MUTED}" font-size="15" text-anchor="end">'
        "six signals, one way to read them: signal.sample(times)</text>\n</g>\n</svg>\n"
    )
    out = ROOT / "docs" / "images" / "banner.svg"
    out.write_text(svg, encoding="utf-8")
    print(out.relative_to(ROOT), f"{out.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
