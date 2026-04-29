#!/usr/bin/env python3

import argparse
import math
import pathlib
import struct
import zlib


def parse_args():
    parser = argparse.ArgumentParser(
        description="Generate voxel-field test images for the field viewer."
    )
    parser.add_argument("--x", type=int, default=64, help="Voxel resolution along X.")
    parser.add_argument("--y", type=int, default=64, help="Voxel resolution along Y.")
    parser.add_argument("--z", type=int, default=16, help="Voxel resolution along Z.")
    parser.add_argument(
        "--output-dir",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent / "output",
        help="Directory for generated PNG files.",
    )
    parser.add_argument(
        "--cols",
        type=int,
        default=0,
        help="Number of columns in the slice atlas. Defaults to a near-square layout.",
    )
    return parser.parse_args()


def validate_args(args):
    for name in ("x", "y", "z"):
        if getattr(args, name) < 1:
            raise SystemExit(f"--{name} must be >= 1")
    if args.cols < 0:
        raise SystemExit("--cols must be >= 0")


def choose_grid(depth, forced_cols):
    if forced_cols > 0:
        cols = forced_cols
    else:
        cols = math.ceil(math.sqrt(depth))
    rows = math.ceil(depth / cols)
    return cols, rows


def voxel_center(index, resolution):
    return ((index + 0.5) / resolution) * 2.0 - 1.0


def sphere_sdf(x, y, z):
    radius = 0.58
    return math.sqrt(x * x + y * y + z * z) - radius


def cone_sdf(x, y, z):
    base_radius = 0.75
    half_height = 0.7
    radial = math.sqrt(x * x + z * z)
    y_centered = y

    # Exact SDF for a capped cone with base radius r1 at y=-h and tip radius r2 at y=+h.
    r1 = base_radius
    r2 = 0.0
    qx = radial
    qy = y_centered
    k1x = r2
    k1y = half_height
    k2x = r2 - r1
    k2y = 2.0 * half_height

    if qy < 0.0:
        clamped_radius = r1
    else:
        clamped_radius = r2
    cax = qx - min(qx, clamped_radius)
    cay = abs(qy) - half_height

    k1_minus_qx = k1x - qx
    k1_minus_qy = k1y - qy
    k2_dot = k2x * k2x + k2y * k2y
    projection = 0.0
    if k2_dot > 0.0:
        projection = max(
            0.0,
            min(1.0, (k1_minus_qx * k2x + k1_minus_qy * k2y) / k2_dot),
        )
    cbx = qx - k1x + k2x * projection
    cby = qy - k1y + k2y * projection

    ca_distance_squared = cax * cax + cay * cay
    cb_distance_squared = cbx * cbx + cby * cby
    sign = -1.0 if cbx < 0.0 and cay < 0.0 else 1.0
    return sign * math.sqrt(min(ca_distance_squared, cb_distance_squared))


def quantize_signed_distance(distance, max_distance=1.25):
    normalized = (distance / max_distance) * 0.5 + 0.5
    clamped = min(1.0, max(0.0, normalized))
    return int(round(clamped * 255.0))


def encode_voxel(distance):
    inside = distance <= 0.0
    sdf_channel = quantize_signed_distance(distance)
    occupancy = 255 if inside else 0
    inverse = 255 - occupancy
    return (sdf_channel, occupancy, inverse, 255)


def build_atlas(width, height, depth, cols, rows, sdf_fn):
    atlas_width = width * cols
    atlas_height = height * rows
    pixels = bytearray(atlas_width * atlas_height * 4)

    for z_index in range(depth):
        cell_col = z_index % cols
        cell_row = z_index // cols

        z_pos = voxel_center(z_index, depth)

        for y_index in range(height):
            y_pos = voxel_center(y_index, height)
            atlas_y = cell_row * height + y_index

            for x_index in range(width):
                x_pos = voxel_center(x_index, width)
                atlas_x = cell_col * width + x_index

                distance = sdf_fn(x_pos, y_pos, z_pos)
                r, g, b, a = encode_voxel(distance)
                offset = (atlas_y * atlas_width + atlas_x) * 4
                pixels[offset : offset + 4] = bytes((r, g, b, a))

    return atlas_width, atlas_height, pixels


def png_chunk(chunk_type, data):
    payload = chunk_type + data
    checksum = zlib.crc32(payload) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + payload + struct.pack(">I", checksum)


def write_png(path, width, height, rgba_bytes):
    header = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)

    scanlines = bytearray()
    stride = width * 4
    for row in range(height):
        start = row * stride
        scanlines.append(0)
        scanlines.extend(rgba_bytes[start : start + stride])

    compressed = zlib.compress(bytes(scanlines), level=9)
    png = bytearray()
    png.extend(header)
    png.extend(png_chunk(b"IHDR", ihdr))
    png.extend(png_chunk(b"IDAT", compressed))
    png.extend(png_chunk(b"IEND", b""))
    path.write_bytes(png)


def main():
    args = parse_args()
    validate_args(args)
    args.output_dir.mkdir(parents=True, exist_ok=True)

    cols, rows = choose_grid(args.z, args.cols)
    shapes = {
        "sphere": sphere_sdf,
        "cone": cone_sdf,
    }

    print(f"Voxel resolution: {args.x} x {args.y} x {args.z}")
    print(f"Atlas grid: {cols} x {rows}")
    print(f"Output directory: {args.output_dir}")

    for name, sdf_fn in shapes.items():
        atlas_width, atlas_height, pixels = build_atlas(
            args.x, args.y, args.z, cols, rows, sdf_fn
        )
        output_path = args.output_dir / f"{name}.png"
        write_png(output_path, atlas_width, atlas_height, pixels)
        print(f"Wrote {output_path.name}: {atlas_width} x {atlas_height}")


if __name__ == "__main__":
    main()
