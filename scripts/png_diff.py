"""Pixel difference of two PNG pictures of the same size, for scripts/golden.sh.

    python3 -I scripts/png_diff.py a.png b.png [diff.png [allowed %]]

Prints the share of pixels that differ and the largest and mean channel difference (0..255,
over every channel of every pixel), and writes the difference amplified 8 times into diff.png.
Exits 1 when more than the allowed share of pixels (default 0) differs.
Only the standard library: a small PNG reader for the 8-bit, non-interlaced RGB/RGBA files
openbus writes.
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        sys.exit(f"{path}: not a PNG")
    pos, idat, w = 8, [], 0
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            w, h, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or color not in (2, 6) or interlace:
                sys.exit(f"{path}: only 8-bit RGB/RGBA non-interlaced PNGs (depth {depth}, colour {color})")
            bpp = 3 if color == 2 else 4
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    raw = zlib.decompress(b"".join(idat))
    stride = w * bpp
    rows, prev = [], bytearray(stride)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        if f == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif f == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif f == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif f == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 255
        rows.append(line)
        prev = line
    # drop alpha: the pictures are opaque, and RGB keeps the numbers comparable
    rgb = bytearray()
    for line in rows:
        if bpp == 4:
            for i in range(0, stride, 4):
                rgb += line[i:i + 3]
        else:
            rgb += line
    return w, h, rgb


def write_png(path, w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb[y * w * 3:(y + 1) * w * 3]) for y in range(h))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    with open(path, "wb") as out:
        out.write(b"\x89PNG\r\n\x1a\n")
        out.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)))
        out.write(chunk(b"IDAT", zlib.compress(raw, 6)))
        out.write(chunk(b"IEND", b""))


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    wa, ha, a = read_png(sys.argv[1])
    wb, hb, b = read_png(sys.argv[2])
    if (wa, ha) != (wb, hb):
        print(f"size differs: {wa}x{ha} vs {wb}x{hb}")
        sys.exit(1)
    diff = bytearray(abs(x - y) for x, y in zip(a, b))
    pixels = sum(1 for i in range(0, len(diff), 3) if diff[i] or diff[i + 1] or diff[i + 2])
    share = 100.0 * pixels / (wa * ha)
    allowed = float(sys.argv[4]) if len(sys.argv) > 4 else 0.0
    verdict = "differs" if share > allowed else f"within its {allowed:g} % allowance"
    print(f"{verdict}: {share:.3f} % of pixels, max {max(diff)}, mean {sum(diff) / len(diff):.4f}")
    if len(sys.argv) > 3:
        write_png(sys.argv[3], wa, ha, bytearray(min(255, d * 8) for d in diff))
    if share > allowed:
        sys.exit(1)


main()
