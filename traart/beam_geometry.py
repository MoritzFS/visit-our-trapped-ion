"""Beam geometry at the ion, top view and cross-section.

Everything lives in the horizontal plane through the ion except the axial
pair, which runs along the trap axis (also in that plane).  Directions follow
the lab's own schematic:

    z  trap axis (horizontal);  B along z, about 5 G
    x  in the horizontal plane, perpendicular to z;  y  lab vertical
    397 sigma + 854   along +z (axial, along B)
    397 pi   + 866    at 45 deg to z, in the horizontal plane, from +z +x
    729 addressing    along x through the objective (NA 0.4), 90 deg to z,
                      AOD-steered along z ("vertical") and y ("horizontal")
    729 diagonal      at 45 deg, 90 deg from the 397 pi beam, from -z +x

Edit BEAMS to move anything.  The cross-section's trap structure is schematic:
a monolithic segmented trap drawn as a slotted wafer so that every in-plane
beam passes through the slot.
"""

from __future__ import annotations

import math

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Circle, Ellipse, FancyArrowPatch, Polygon, Rectangle

# key, label, colour, azimuth from +z in the horizontal plane (deg), lw, note
# azimuth is the direction the light travels *from* (where the arrow's tail is).
BEAMS = [
    ("397_sigma", "397 σ",         "#7A3FA8", 180.0, 3.2, "axial, along B: EIT cooling, state prep"),
    ("854",       "854",           "#7A2230", 180.0, 3.2, "axial: recoil along the axial mode"),
    ("397_pi",    "397 π",         "#4B2FBF",  45.0, 3.2, "detection, Doppler cooling"),
    ("866",       "866",           "#5E2050",  45.0, 3.2, "repumper, with 397 π"),
    ("729_addr",  "729",           "#E0362A",  -90.0, 5.0, "addressing, via objective, AOD-steered"),
    ("729_diag",  "729 diagonal",  "#B0221A",  135.0, 2.4, "global beam, 90° from 397 π"),
]

INK, DIM = "#1A1A1A", "#6B6B6B"
CHAMBER, CHAMBER_EDGE = "#ECEAE5", "#8A8680"
ELECTRODE, ELECTRODE_EDGE = "#C9B458", "#6E5D1E"
ION_Z = 0.4          # the ion sits ~500 um from the trap centre along z


def arrow(ax, tail, head, colour, lw, ls="-", z=8):
    ax.add_patch(FancyArrowPatch(tail, head, arrowstyle="-|>", mutation_scale=14 + 5 * lw,
                                 color=colour, lw=lw, linestyle=ls, zorder=z,
                                 shrinkA=0, shrinkB=0))


def top_view(ax):
    ax.set_aspect("equal")
    ax.set_xlim(-7.2, 7.2)
    ax.set_ylim(-5.6, 5.9)
    ax.axis("off")
    ax.text(0.0, 1.0, "Horizontal plane through the ion — top view", fontsize=12.5,
            color=INK, family="serif", transform=ax.transAxes, va="top")

    # vacuum chamber: chamfered corners carry the 45° windows, a re-entrant
    # viewport at the bottom holds the objective
    chamber = [(-4.2, -2.4), (-0.7, -2.4), (-0.7, -1.7), (1.5, -1.7), (1.5, -2.4),
               (4.2, -2.4), (4.2, 0.6), (1.6, 3.2), (-1.6, 3.2), (-4.2, 0.6)]
    ax.add_patch(Polygon(chamber, closed=True, facecolor=CHAMBER, edgecolor=CHAMBER_EDGE,
                         lw=1.6, zorder=1))
    ax.text(-4.05, 0.85, "vacuum chamber", fontsize=8, color=DIM, family="serif", rotation=45)

    # trap: segmented electrodes either side of the slot, along z
    ax.add_patch(Rectangle((-2.4, -0.42), 4.8, 0.84, facecolor="#EFE6C2",
                           edgecolor=ELECTRODE_EDGE, lw=1.0, zorder=2))
    for side in (+1, -1):
        y0 = 0.10 if side > 0 else -0.34
        ax.add_patch(Rectangle((-2.3, y0), 4.6, 0.24, facecolor=ELECTRODE,
                               edgecolor=ELECTRODE_EDGE, lw=0.6, zorder=3))
        for zz in [-2.3 + 4.6 * k / 9 for k in range(1, 9)]:
            ax.plot([zz, zz], [y0, y0 + 0.24], color=ELECTRODE_EDGE, lw=0.6, zorder=4)
    ax.plot([-2.3, 2.3], [0, 0], color=DIM, lw=0.6, ls=(0, (2, 2)), zorder=4)
    ax.text(-2.4, -0.62, "monolithic segmented trap", fontsize=8, color=ELECTRODE_EDGE,
            family="serif", va="top")

    # objective in the re-entrant viewport, below the trap
    ax.add_patch(Ellipse((ION_Z, -2.05), 1.9, 0.42, facecolor="#C9D3E3", edgecolor=INK,
                         lw=1.0, zorder=5))
    ax.text(1.65, -2.05, "objective\nNA 0.4, re-entrant viewport", fontsize=8, color=INK,
            family="serif", va="center")

    # ion
    ax.add_patch(Circle((ION_Z, 0), 0.11, facecolor="#1E40AF", edgecolor="white", lw=0.8,
                        zorder=10))
    ax.text(ION_Z + 0.85, -0.62, "⁴⁰Ca⁺, ≈500 µm from trap centre", fontsize=8.5, color=INK,
            family="serif", va="top", zorder=10)

    # beams, grouped by direction so co-propagating pairs sit side by side
    groups: dict[float, list] = {}
    for b in BEAMS:
        groups.setdefault(b[3], []).append(b)
    for az, members in groups.items():
        a = math.radians(az)
        dz, dx = math.cos(a), math.sin(a)          # unit vector *from* the source
        pz, px = -dx, dz                            # perpendicular
        L_tail = 4.3 if abs(az) != 90 else 4.6
        for i, (key, label, col, _, lw, note) in enumerate(members):
            off = 0.0 if len(members) == 1 else (0.18 if i == 0 else -0.18)
            tail = (ION_Z + dz * L_tail + pz * off, dx * L_tail + px * off)
            head = (ION_Z + dz * 0.45 + pz * off, dx * 0.45 + px * off)
            arrow(ax, tail, head, col, lw)
        # one label block per direction, just beyond the tails
        names = "\n".join(m[1] for m in members)
        deg = f"{abs(az) if az != 180 else 0:g}° to z" if az != -90 else "90° to z"
        lz, lx = ION_Z + dz * (L_tail + 0.35), dx * (L_tail + 0.35)
        ha = "right" if dz < -0.3 else ("left" if dz > 0.3 else "center")
        va = "bottom" if dx > 0.3 else ("top" if dx < -0.3 else "center")
        col = members[0][2]
        ax.text(lz, lx, names, fontsize=13, color=col, family="serif", ha=ha, va=va,
                linespacing=1.05, zorder=11)
        # small angle tag
        tz, tx = ION_Z + dz * (L_tail - 0.9) + pz * (0.55 if az != -90 else 0.0), \
            dx * (L_tail - 0.9) + px * (0.55 if az != -90 else 0.0)
        if az == -90:
            tz += 0.9
        ax.text(tz, tx, deg, fontsize=8, color=DIM, family="serif", ha="left", va="center")

    # angle arc: 397 pi at 45 deg to the axis
    r = 1.35
    th = [math.radians(t) for t in range(0, 46, 3)]
    ax.plot([ION_Z + r * math.cos(t) for t in th], [r * math.sin(t) for t in th],
            color=DIM, lw=0.7, zorder=6)
    ax.text(ION_Z + 1.5, 0.35, "45°", fontsize=8, color=DIM, family="serif")
    # 90 deg between 397 pi and the 729 diagonal
    r = 2.1
    th = [math.radians(t) for t in range(45, 136, 3)]
    ax.plot([ION_Z + r * math.cos(t) for t in th], [r * math.sin(t) for t in th],
            color=DIM, lw=0.7, zorder=6)
    ax.text(ION_Z, r + 0.12, "90°", fontsize=8, color=DIM, family="serif", ha="center")

    # AOD steering note by the 729 tail
    ax.text(ION_Z - 0.6, -4.95,
            "AOD “vertical” → along z\nAOD “horizontal” → along y (⊙)",
            fontsize=8.5, color="#E0362A", family="serif", ha="right", va="center")

    # B field and axes, bottom right
    arrow(ax, (3.0, -4.3), (4.6, -4.3), INK, 1.4)
    ax.text(3.0, -4.05, "B ∥ z   (≈ 5 G)", fontsize=9.5, color=INK, family="serif")
    ox, oy = 6.1, -4.3
    arrow(ax, (ox, oy), (ox + 0.9, oy), DIM, 1.0)
    arrow(ax, (ox, oy), (ox, oy + 0.9), DIM, 1.0)
    ax.add_patch(Circle((ox, oy), 0.11, facecolor="none", edgecolor=DIM, lw=1.0))
    ax.add_patch(Circle((ox, oy), 0.03, facecolor=DIM, edgecolor=DIM))
    ax.text(ox + 1.0, oy, "z", fontsize=10, color=DIM, family="serif", va="center", style="italic")
    ax.text(ox, oy + 1.0, "x", fontsize=10, color=DIM, family="serif", ha="center", style="italic")
    ax.text(ox - 0.2, oy - 0.05, "y", fontsize=10, color=DIM, family="serif", ha="right",
            va="top", style="italic")


def cross_section(ax):
    ax.set_aspect("equal")
    ax.set_xlim(-5.4, 4.6)
    ax.set_ylim(-4.2, 4.6)
    ax.axis("off")
    ax.text(0.0, 1.0, "Cross-section ⊥ trap axis — looking along z", fontsize=12.5,
            color=INK, family="serif", transform=ax.transAxes, va="top")

    # trap as a slotted wafer: electrodes above and below the slot, ion in the slot
    for y0, h in ((0.36, 2.6), (-2.96, 2.6)):
        ax.add_patch(Rectangle((-0.42, y0), 0.84, h, facecolor="#EFE6C2",
                               edgecolor=ELECTRODE_EDGE, lw=1.0, zorder=2))
        ax.add_patch(Rectangle((-0.34, y0 + (0.1 if y0 > 0 else h - 0.5)), 0.68, 0.4,
                               facecolor=ELECTRODE, edgecolor=ELECTRODE_EDGE, lw=0.6, zorder=3))
    ax.add_patch(Rectangle((-0.6, 2.96), 1.2, 0.22, facecolor=ELECTRODE,
                           edgecolor=ELECTRODE_EDGE, lw=0.6, zorder=3))
    ax.text(0.75, 3.05, "shims", fontsize=8, color=ELECTRODE_EDGE, family="serif", va="center")
    ax.text(0.6, 1.6, "segmented\nelectrodes\n(schematic)", fontsize=8, color=ELECTRODE_EDGE,
            family="serif", va="center")
    ax.add_patch(Circle((0, 0), 0.11, facecolor="#1E40AF", edgecolor="white", lw=0.8, zorder=10))

    # objective on the -x side, 729 through it, fluorescence back into it
    ax.add_patch(Ellipse((-3.3, 0), 0.42, 1.9, facecolor="#C9D3E3", edgecolor=INK, lw=1.0,
                         zorder=5))
    ax.text(-3.3, 1.15, "objective", fontsize=9, color=INK, family="serif", ha="center")
    arrow(ax, (-5.1, 0), (-0.55, 0), "#E0362A", 5.0)
    ax.text(-3.3, -1.25, "729 addressing", fontsize=12, color="#E0362A", family="serif",
            ha="center", va="top")
    for dy in (-0.55, 0.55):
        ax.plot([-0.25, -3.1], [0.05 * (1 if dy > 0 else -1), dy], color="#7A3FA8", lw=0.7,
                ls=(0, (2, 3)), zorder=6)
    ax.text(-1.6, 0.95, "fluorescence to camera", fontsize=8, color="#7A3FA8", family="serif",
            ha="center")
    # AOD "horizontal" = y
    ax.add_patch(FancyArrowPatch((-2.4, -1.5), (-2.4, -2.5), arrowstyle="<|-|>",
                                 mutation_scale=10, color="#E0362A", lw=1.0))
    ax.text(-2.25, -2.0, "AOD “horizontal”\nsteers along y", fontsize=8, color="#E0362A",
            family="serif", va="center")

    # in-plane 45° beams arrive from the +x side, seen edge-on
    arrow(ax, (4.2, 0.0), (0.55, 0.0), "#4B2FBF", 2.6)
    ax.text(2.4, 0.3, "397 π + 866  and  729 diagonal", fontsize=9.5, color="#4B2FBF",
            family="serif", ha="center", va="bottom")
    ax.text(2.4, -0.3, "both at 45° to z in the horizontal plane, seen edge-on",
            fontsize=8, color=DIM, family="serif", ha="center", va="top")

    # axial pair and B: out of the page
    ax.add_patch(Circle((2.6, -3.0), 0.17, facecolor="none", edgecolor=INK, lw=1.2))
    ax.add_patch(Circle((2.6, -3.0), 0.035, facecolor=INK, edgecolor=INK))
    ax.text(2.85, -3.0, "z, B, 397 σ, 854\nout of the page", fontsize=9, color=INK,
            family="serif", va="center")
    # axes
    ox, oy = -4.9, -3.3
    arrow(ax, (ox, oy), (ox + 0.9, oy), DIM, 1.0)
    arrow(ax, (ox, oy), (ox, oy + 0.9), DIM, 1.0)
    ax.text(ox + 1.0, oy, "x", fontsize=10, color=DIM, family="serif", va="center", style="italic")
    ax.text(ox, oy + 1.0, "y  (lab vertical)", fontsize=10, color=DIM, family="serif",
            ha="left", style="italic")
    ax.text(-5.2, -4.0, "The slot runs along z, so every in-plane beam passes through it.",
            fontsize=8, color=DIM, family="serif")


def main(out_svg="out/beam_geometry.svg", out_png="out/beam_geometry.png"):
    plt.rcParams["font.family"] = "serif"
    fig, (a, b) = plt.subplots(1, 2, figsize=(15.5, 7.2), gridspec_kw={"width_ratios": [1.45, 1]})
    fig.patch.set_facecolor("white")
    top_view(a)
    cross_section(b)
    fig.text(0.01, 0.012,
             "Directions from the lab schematic (Fig. S1): axial pair along B; 397 π + 866 at 45°; "
             "729 through the objective at 90°; 729 diagonal 90° from 397 π.  "
             "Trap cross-section is schematic.  Not to scale.",
             fontsize=8.5, color=DIM, family="serif")
    fig.tight_layout(rect=(0, 0.03, 1, 1))
    fig.savefig(out_svg)
    fig.savefig(out_png, dpi=170)
    return out_svg, out_png


if __name__ == "__main__":
    print(main())
