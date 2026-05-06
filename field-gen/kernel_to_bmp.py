#!/usr/bin/env python3
"""Convert an explicit JSON growth kernel into a BMP slice atlas.

The BMP uses the same channel convention as field-gen atlas previews:
R stores normalized cost, G stores the active kernel mask, and B is zero.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Convert an explicit growth kernel JSON file to a BMP atlas."
    )
    parser.add_argument("kernel", type=Path, help="Input explicit kernel JSON path.")
    parser.add_argument(
        "--output",
        "-o",
        type=Path,
        default=None,
        help="Output BMP path. Defaults to the input path with .bmp extension.",
    )
    parser.add_argument(
        "--metadata",
        type=Path,
        default=None,
        help="Optional output metadata JSON path. Defaults to <bmp-stem>.metadata.json.",
    )
    parser.add_argument(
        "--no-metadata",
        action="store_true",
        help="Do not write a companion metadata JSON file.",
    )
    parser.add_argument(
        "--include-center",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Mark the kernel origin as occupied with zero cost for visualization.",
    )
    return parser.parse_args()


def load_kernel(path: Path) -> dict:
    kernel = json.loads(path.read_text(encoding="utf-8"))
    if kernel.get("type") != "explicit":
        raise ValueError("only explicit kernels are supported")
    if not isinstance(kernel.get("moves"), list):
        raise ValueError("kernel must contain a moves array")
    return kernel


def infer_dimensions(kernel: dict) -> tuple[int, int, int]:
    dimensions = kernel.get("dimensions")
    if dimensions is not None:
        if (
            not isinstance(dimensions, list)
            or len(dimensions) != 3
            or any(not isinstance(value, int) or value <= 0 for value in dimensions)
        ):
            raise ValueError("dimensions must be three positive integers")
        return tuple(dimensions)

    radius = 0
    for move in kernel["moves"]:
        offset = move.get("offset")
        if not isinstance(offset, list) or len(offset) != 3:
            raise ValueError("each move must have a three-value offset")
        radius = max(radius, *(abs(int(value)) for value in offset))

    size = radius * 2 + 1
    return (size, size, size)


def build_volume(
    kernel: dict, dimensions: tuple[int, int, int], include_center: bool
) -> tuple[list[float], list[int], float]:
    width, height, depth = dimensions
    origin = [axis // 2 for axis in dimensions]
    values = [math.inf] * (width * height * depth)
    mask = [0] * (width * height * depth)
    max_cost = 0.0

    if include_center:
        center = index(origin[0], origin[1], origin[2], dimensions)
        values[center] = 0.0
        mask[center] = 255

    for move in kernel["moves"]:
        offset = move.get("offset")
        cost = move.get("cost")
        if not isinstance(offset, list) or len(offset) != 3:
            raise ValueError("each move must have a three-value offset")
        if not isinstance(cost, (int, float)) or cost < 0.0 or not math.isfinite(cost):
            raise ValueError("each move must have a finite non-negative cost")

        x = origin[0] + int(offset[0])
        y = origin[1] + int(offset[1])
        z = origin[2] + int(offset[2])
        if x < 0 or y < 0 or z < 0 or x >= width or y >= height or z >= depth:
            raise ValueError(f"move offset {offset} falls outside dimensions {dimensions}")

        voxel = index(x, y, z, dimensions)
        values[voxel] = min(values[voxel], float(cost))
        mask[voxel] = 255
        max_cost = max(max_cost, float(cost))

    return values, mask, max_cost


def build_atlas(dimensions: tuple[int, int, int]) -> tuple[int, int, int, int]:
    width, height, depth = dimensions
    columns = math.ceil(math.sqrt(depth))
    rows = math.ceil(depth / columns)
    return columns, rows, width * columns, height * rows


def write_bmp(
    path: Path,
    dimensions: tuple[int, int, int],
    values: list[float],
    mask: list[int],
    max_cost: float,
) -> tuple[int, int, int, int]:
    width, height, depth = dimensions
    columns, rows, atlas_width, atlas_height = build_atlas(dimensions)
    row_stride = ((atlas_width * 3 + 3) // 4) * 4
    pixel_data_size = row_stride * atlas_height
    file_size = 14 + 40 + pixel_data_size
    padding = row_stride - atlas_width * 3

    data = bytearray()
    data.extend(b"BM")
    data.extend(struct.pack("<IHHI", file_size, 0, 0, 54))
    data.extend(
        struct.pack(
            "<IiiHHIIiiII",
            40,
            atlas_width,
            -atlas_height,
            1,
            24,
            0,
            pixel_data_size,
            2835,
            2835,
            0,
            0,
        )
    )

    for atlas_y in range(atlas_height):
        for atlas_x in range(atlas_width):
            slice_column = atlas_x // width
            slice_row = atlas_y // height
            z = slice_row * columns + slice_column
            x = atlas_x % width
            y = atlas_y % height
            red = 0
            green = 0
            blue = 0

            if z < depth:
                voxel = index(x, y, z, dimensions)
                green = mask[voxel]
                if green and max_cost > 0.0 and math.isfinite(values[voxel]):
                    red = round(max(0.0, min(1.0, values[voxel] / max_cost)) * 255.0)

            data.extend(bytes((blue, green, red)))

        data.extend(b"\x00" * padding)

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return columns, rows, atlas_width, atlas_height


def write_metadata(
    path: Path,
    kernel_path: Path,
    image_path: Path,
    kernel: dict,
    dimensions: tuple[int, int, int],
    atlas: tuple[int, int, int, int],
    max_cost: float,
) -> None:
    columns, rows, atlas_width, atlas_height = atlas
    metadata = {
        "input": str(kernel_path),
        "kernel_name": kernel.get("name"),
        "units": kernel.get("units", "voxels"),
        "layout": "x-fastest-u8",
        "image_file": str(image_path),
        "image_format": "bmp-r-field-g-occupancy-slice-atlas",
        "image_grid": [columns, rows],
        "image_size_px": [atlas_width, atlas_height],
        "dimensions": list(dimensions),
        "field_enabled": True,
        "field_max_distance": max_cost,
        "channel_layout": {
            "r": "normalized kernel cost",
            "g": "active kernel entry mask",
            "b": "reserved",
        },
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")


def index(x: int, y: int, z: int, dimensions: tuple[int, int, int]) -> int:
    width, height, _ = dimensions
    return x + y * width + z * width * height


def main() -> None:
    args = parse_args()
    kernel = load_kernel(args.kernel)
    dimensions = infer_dimensions(kernel)
    values, mask, max_cost = build_volume(kernel, dimensions, args.include_center)
    output = args.output or args.kernel.with_suffix(".bmp")
    atlas = write_bmp(output, dimensions, values, mask, max_cost)
    print(f"Wrote {output}")

    if not args.no_metadata:
        metadata = args.metadata or output.with_name(f"{output.stem}.metadata.json")
        write_metadata(metadata, args.kernel, output, kernel, dimensions, atlas, max_cost)
        print(f"Wrote {metadata}")


if __name__ == "__main__":
    main()
