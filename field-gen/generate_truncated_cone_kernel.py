#!/usr/bin/env python3
"""Generate an explicit JSON growth kernel whose unit threshold is a frustum.

By default, field-distance <= 1 describes a truncated cone centered around the
kernel origin, spanning z=-1mm to z=+1mm. The bottom diameter is 3mm and the top
diameter is 1mm. Samples beyond that unit surface are also emitted so the kernel
represents a continuous field around the threshold shape.
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate an explicit truncated-cone growth kernel."
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
        "--z-min",
        type=float,
        default=-1.0,
        help="Bottom of the unit frustum relative to the kernel center, in mm.",
    )
    parser.add_argument(
        "--z-max",
        type=float,
        default=1.0,
        help="Top of the unit frustum relative to the kernel center, in mm.",
    )
    parser.add_argument(
        "--bottom-diameter",
        type=positive_float,
        default=3.0,
        help="Unit frustum bottom diameter in mm.",
    )
    parser.add_argument(
        "--top-diameter",
        type=positive_float,
        default=1.0,
        help="Unit frustum top diameter in mm.",
    )
    parser.add_argument(
        "--max-cost",
        type=positive_float,
        default=2.0,
        help="Emit offsets with cost <= this value. The frustum surface remains at cost 1.",
    )
    parser.add_argument(
        "--name",
        default="truncated-cone-unit",
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
        default=Path("truncated-cone-kernel.json"),
        help="Output JSON path.",
    )
    return parser.parse_args()


def frustum_cost(
    x_mm: float,
    y_mm: float,
    z_mm: float,
    z_min_mm: float,
    z_max_mm: float,
    bottom_radius_mm: float,
    top_radius_mm: float,
) -> float:
    """Return the scale factor whose frustum boundary passes through a point."""
    if z_mm >= 0.0:
        vertical_cost = z_mm / z_max_mm
    else:
        vertical_cost = z_mm / z_min_mm

    slope = (top_radius_mm - bottom_radius_mm) / (z_max_mm - z_min_mm)
    center_radius = bottom_radius_mm - slope * z_min_mm
    radial_cost = (math.hypot(x_mm, y_mm) - slope * z_mm) / center_radius
    return max(vertical_cost, radial_cost)


def build_kernel(args: argparse.Namespace) -> dict:
    if args.z_min >= 0.0 or args.z_max <= 0.0 or args.z_min >= args.z_max:
        raise ValueError("--z-min must be below zero and --z-max must be above zero")

    voxel_size = tuple(float(value) for value in args.voxel)
    bottom_radius = args.bottom_diameter / 2.0
    top_radius = args.top_diameter / 2.0
    max_radius = max(bottom_radius, top_radius) * args.max_cost

    radius_x = math.ceil(max_radius / voxel_size[0])
    radius_y = math.ceil(max_radius / voxel_size[1])
    radius_z = math.ceil(max(abs(args.z_min), abs(args.z_max)) * args.max_cost / voxel_size[2])
    dimensions = [radius_x * 2 + 1, radius_y * 2 + 1, radius_z * 2 + 1]
    moves = []

    for dz in range(-radius_z, radius_z + 1):
        for dy in range(-radius_y, radius_y + 1):
            for dx in range(-radius_x, radius_x + 1):
                if dx == 0 and dy == 0 and dz == 0:
                    continue

                x_mm = dx * voxel_size[0]
                y_mm = dy * voxel_size[1]
                z_mm = dz * voxel_size[2]
                cost = frustum_cost(
                    x_mm,
                    y_mm,
                    z_mm,
                    args.z_min,
                    args.z_max,
                    bottom_radius,
                    top_radius,
                )
                if cost <= args.max_cost:
                    moves.append({"offset": [dx, dy, dz], "cost": cost})

    return {
        "version": 1,
        "type": "explicit",
        "name": args.name,
        "units": "voxels",
        "path_check": args.path_check,
        "voxel_size_mm": list(voxel_size),
        "unit_shape": {
            "type": "truncated_cone",
            "z_min_mm": args.z_min,
            "z_max_mm": args.z_max,
            "bottom_diameter_mm": args.bottom_diameter,
            "top_diameter_mm": args.top_diameter,
            "threshold": 1.0,
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
