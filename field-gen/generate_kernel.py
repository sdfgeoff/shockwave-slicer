#!/usr/bin/env python3
"""Generate an explicit JSON growth kernel.

The default output is a 7x7x7 ellipsoidal kernel matching the current
anisotropic Euclidean propagation model, but encoded as explicit offsets.
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


def non_negative_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be greater than or equal to zero")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate an explicit JSON ellipsoidal growth kernel."
    )
    parser.add_argument(
        "--radius",
        type=non_negative_int,
        default=3,
        help="Kernel radius in voxels. Radius 3 produces a 7x7x7 kernel.",
    )
    parser.add_argument(
        "--voxel",
        type=positive_float,
        nargs=3,
        default=(1.0, 1.0, 1.0),
        metavar=("X", "Y", "Z"),
        help="Voxel size in mm used for cost generation.",
    )
    parser.add_argument(
        "--rate",
        type=positive_float,
        nargs=3,
        default=(3.7, 3.7, 1.0),
        metavar=("X", "Y", "Z"),
        help="Axis propagation rates. Higher values make movement cheaper.",
    )
    parser.add_argument(
        "--name",
        default=None,
        help="Kernel name. Defaults to ellipsoid-<size>x<size>x<size>.",
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
        default=Path("kernel.json"),
        help="Output JSON path.",
    )
    return parser.parse_args()


def movement_cost(
    offset: tuple[int, int, int],
    voxel_size_mm: tuple[float, float, float],
    rate: tuple[float, float, float],
) -> float:
    dx, dy, dz = offset
    vx, vy, vz = voxel_size_mm
    rx, ry, rz = rate
    return math.sqrt(
        ((dx * vx) / rx) ** 2
        + ((dy * vy) / ry) ** 2
        + ((dz * vz) / rz) ** 2
    )


def build_kernel(args: argparse.Namespace) -> dict:
    radius = args.radius
    size = radius * 2 + 1
    voxel_size_mm = tuple(float(value) for value in args.voxel)
    rate = tuple(float(value) for value in args.rate)
    moves = []

    for dz in range(-radius, radius + 1):
        for dy in range(-radius, radius + 1):
            for dx in range(-radius, radius + 1):
                if dx == 0 and dy == 0 and dz == 0:
                    continue

                moves.append(
                    {
                        "offset": [dx, dy, dz],
                        "cost": movement_cost((dx, dy, dz), voxel_size_mm, rate),
                    }
                )

    return {
        "version": 1,
        "type": "explicit",
        "name": args.name or f"ellipsoid-{size}x{size}x{size}",
        "units": "voxels",
        "path_check": args.path_check,
        "voxel_size_mm": list(voxel_size_mm),
        "rate": list(rate),
        "radius_voxels": radius,
        "dimensions": [size, size, size],
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
