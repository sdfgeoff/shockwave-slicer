#!/usr/bin/env python3
"""Generate an explicit radial kernel from Inigo Quilez's trapezoid SDF.

The source SDF is 2D. Shader X maps to radial distance sqrt(x*x + y*y), and
shader Y maps to field Z. The default parameters match the supplied Shadertoy
snippet:

    field = sdTrapezoid(vec2(radial, z) + vec2(0, 0.5), 2.0, 0.2, 0.5)

The signed distance is shifted by --surface-cost so the zero SDF contour maps to
that field-distance. By default, the contour is field-distance 1.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def positive_float(value: str) -> float:
    parsed = float(value)
    if parsed <= 0.0 or not math.isfinite(parsed):
        raise argparse.ArgumentTypeError("value must be a finite number greater than zero")
    return parsed


def finite_float(value: str) -> float:
    parsed = float(value)
    if not math.isfinite(parsed):
        raise argparse.ArgumentTypeError("value must be finite")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate an explicit radial kernel from a trapezoid signed-distance field."
    )
    parser.add_argument(
        "--voxel",
        type=positive_float,
        nargs=3,
        default=(1.0, 1.0, 1.0),
        metavar=("X", "Y", "Z"),
        help="Voxel size in mm used to sample the kernel.",
    )
    parser.add_argument(
        "--r1",
        type=positive_float,
        default=2.0,
        help="Bottom radius parameter passed to sdTrapezoid.",
    )
    parser.add_argument(
        "--r2",
        type=positive_float,
        default=0.2,
        help="Top radius parameter passed to sdTrapezoid.",
    )
    parser.add_argument(
        "--half-height",
        type=positive_float,
        default=0.5,
        help="Half-height parameter passed to sdTrapezoid.",
    )
    parser.add_argument(
        "--z-offset",
        type=finite_float,
        default=0.5,
        help="Offset added to shader Y before evaluating the SDF.",
    )
    parser.add_argument(
        "--surface-cost",
        type=positive_float,
        default=1.0,
        help="Field-distance assigned to the zero SDF contour.",
    )
    parser.add_argument(
        "--max-cost",
        type=positive_float,
        default=2.0,
        help="Only emit offsets with 0 < cost <= this value.",
    )
    parser.add_argument(
        "--margin",
        type=positive_float,
        default=0.0,
        help="Extra bounding-box margin in mm. Defaults to max-cost - surface-cost.",
    )
    parser.add_argument(
        "--name",
        default="trapezoid-sdf-radial",
        help="Kernel name.",
    )
    parser.add_argument(
        "--path-check",
        choices=("endpoint_occupied", "swept_occupied"),
        default="swept_occupied",
        help="Suggested validation mode for long kernel moves.",
    )
    parser.add_argument(
        "--output",
        "-o",
        type=Path,
        default=Path("trapezoid-sdf-kernel.json"),
        help="Output JSON path.",
    )
    return parser.parse_args()


def dot2(v: tuple[float, float]) -> float:
    return v[0] * v[0] + v[1] * v[1]


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


def sd_trapezoid(p: tuple[float, float], r1: float, r2: float, he: float) -> float:
    k1 = (r2, he)
    k2 = (r2 - r1, 2.0 * he)
    px = abs(p[0])
    py = p[1]

    ca = (max(0.0, px - (r1 if py < 0.0 else r2)), abs(py) - he)
    k1_minus_p = (k1[0] - px, k1[1] - py)
    h = clamp((k1_minus_p[0] * k2[0] + k1_minus_p[1] * k2[1]) / dot2(k2), 0.0, 1.0)
    cb = (px - k1[0] + k2[0] * h, py - k1[1] + k2[1] * h)
    sign = -1.0 if cb[0] < 0.0 and ca[1] < 0.0 else 1.0
    return sign * math.sqrt(min(dot2(ca), dot2(cb)))


def build_kernel(args: argparse.Namespace) -> dict:
    voxel_size = tuple(float(value) for value in args.voxel)
    outer_margin = args.margin if args.margin > 0.0 else args.max_cost - args.surface_cost
    if outer_margin < 0.0:
        raise ValueError("--max-cost must be greater than or equal to --surface-cost")

    max_radius_mm = max(args.r1, args.r2) + outer_margin
    min_z_mm = -args.z_offset - args.half_height - outer_margin
    max_z_mm = -args.z_offset + args.half_height + outer_margin
    radius_x = math.ceil(max_radius_mm / voxel_size[0])
    radius_y = math.ceil(max_radius_mm / voxel_size[1])
    radius_z = math.ceil(max(abs(min_z_mm), abs(max_z_mm)) / voxel_size[2])
    dimensions = [radius_x * 2 + 1, radius_y * 2 + 1, radius_z * 2 + 1]
    moves = []

    for dz in range(-radius_z, radius_z + 1):
        for dy in range(-radius_y, radius_y + 1):
            for dx in range(-radius_x, radius_x + 1):
                if dx == 0 and dy == 0 and dz == 0:
                    continue

                radial_mm = math.hypot(dx * voxel_size[0], dy * voxel_size[1])
                z_mm = dz * voxel_size[2]
                sdf = sd_trapezoid(
                    (radial_mm, z_mm + args.z_offset),
                    args.r1,
                    args.r2,
                    args.half_height,
                )
                cost = args.surface_cost + sdf
                if 0.0 < cost <= args.max_cost:
                    moves.append({"offset": [dx, dy, dz], "cost": cost})

    return {
        "version": 1,
        "type": "explicit",
        "name": args.name,
        "units": "voxels",
        "path_check": args.path_check,
        "voxel_size_mm": list(voxel_size),
        "source_field": {
            "type": "radial_trapezoid_sdf",
            "surface_cost": args.surface_cost,
            "max_cost": args.max_cost,
            "r1": args.r1,
            "r2": args.r2,
            "half_height": args.half_height,
            "z_offset": args.z_offset,
            "shader_x": "radial distance in field XY",
            "shader_y": "field Z",
        },
        "dimensions": dimensions,
        "moves": moves,
    }


def main() -> None:
    args = parse_args()
    kernel = build_kernel(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(kernel, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(kernel['moves'])} moves to {args.output}")


if __name__ == "__main__":
    main()
