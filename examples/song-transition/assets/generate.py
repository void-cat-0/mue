#!/usr/bin/env python3
"""Generate three original 960x640 song-transition demo artworks offline.

Copyright (c) 2026 Mue contributors
These original, mathematically generated demo assets and this generator use the
MIT License, the same license as this workspace. No reference artwork, fonts,
third-party copyright assets, network services, or external libraries are used.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
"""

import math
import random
import struct
import zlib
from pathlib import Path

WIDTH, HEIGHT = 960, 640
OUTPUT_DIR = Path(__file__).resolve().parent
COPYRIGHT = b"Copyright (c) 2026 Mue contributors. Original demo artwork. MIT License."


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


class Canvas:
    def __init__(self):
        self.pixels = bytearray(WIDTH * HEIGHT * 3)

    def gradient(self, stops, top=0, bottom=HEIGHT):
        segment = 0
        for y in range(top, bottom):
            while segment + 1 < len(stops) - 1 and y > stops[segment + 1][0]:
                segment += 1
            y0, c0 = stops[segment]
            y1, c1 = stops[segment + 1]
            color = mix(c0, c1, (y - y0) / max(1, y1 - y0))
            start = y * WIDTH * 3
            self.pixels[start:start + WIDTH * 3] = bytes(color) * WIDTH

    def blend(self, x, y, color, alpha=1.0):
        x, y = int(x), int(y)
        if x < 0 or x >= WIDTH or y < 0 or y >= HEIGHT or alpha <= 0:
            return
        alpha = min(1.0, alpha)
        i = (y * WIDTH + x) * 3
        for channel in range(3):
            self.pixels[i + channel] = round(
                self.pixels[i + channel] * (1 - alpha) + color[channel] * alpha
            )

    def glow(self, cx, cy, rx, ry, color, opacity):
        # Only touch the bounded ellipse: one inexpensive pass, not many layers.
        for y in range(max(0, int(cy - ry)), min(HEIGHT, int(cy + ry) + 1)):
            dy2 = ((y - cy) / ry) ** 2
            for x in range(max(0, int(cx - rx)), min(WIDTH, int(cx + rx) + 1)):
                distance = ((x - cx) / rx) ** 2 + dy2
                if distance < 1:
                    self.blend(x, y, color, opacity * (1 - distance) ** 2)

    def disc(self, cx, cy, radius, upper, lower, banding=0.0):
        for y in range(max(0, int(cy - radius - 1)), min(HEIGHT, int(cy + radius + 2))):
            dy = y - cy
            if abs(dy) > radius + 0.5:
                continue
            extent = math.sqrt(max(0, (radius + 0.5) ** 2 - dy * dy))
            t = (dy + radius) / (2 * radius)
            color = mix(upper, lower, t + banding * math.sin(dy / 11))
            for x in range(max(0, int(cx - extent)), min(WIDTH, int(cx + extent + 2))):
                distance = math.hypot(x - cx, dy)
                alpha = min(1.0, max(0.0, radius + 0.5 - distance))
                self.blend(x, y, color, alpha)

    def line(self, x0, y0, x1, y1, color, alpha=1.0, width=1):
        steps = max(1, int(max(abs(x1 - x0), abs(y1 - y0))))
        for step in range(steps + 1):
            t = step / steps
            x, y = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
            ix, iy = int(x), int(y)
            fraction = y - iy
            for offset in range(width):
                self.blend(ix, iy + offset, color, alpha * (1 - fraction))
                self.blend(ix, iy + offset + 1, color, alpha * fraction)

    def polygon(self, vertices, color, lower=None):
        min_y = max(0, math.floor(min(y for _, y in vertices)))
        max_y = min(HEIGHT - 1, math.ceil(max(y for _, y in vertices)))
        edges = list(zip(vertices, vertices[1:] + vertices[:1]))
        for y in range(min_y, max_y + 1):
            intersections = []
            scan_y = y + 0.5
            for (x0, y0), (x1, y1) in edges:
                if (y0 <= scan_y < y1) or (y1 <= scan_y < y0):
                    intersections.append(x0 + (scan_y - y0) * (x1 - x0) / (y1 - y0))
            intersections.sort()
            row_color = color if lower is None else mix(
                color, lower, (y - min_y) / max(1, max_y - min_y)
            )
            for left, right in zip(intersections[0::2], intersections[1::2]):
                x0, x1 = max(0, math.ceil(left)), min(WIDTH, math.ceil(right))
                if x1 > x0:
                    start = (y * WIDTH + x0) * 3
                    self.pixels[start:start + (x1 - x0) * 3] = bytes(row_color) * (x1 - x0)

    def rectangle(self, x0, y0, x1, y1, color):
        self.polygon([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], color)

    def grain(self, seed):
        # A single final pass adds subtle correlated RGB film grain.
        rng = random.Random(seed)
        data = self.pixels
        for i in range(0, len(data), 3):
            noise = rng.randrange(-2, 3)
            data[i] = max(0, min(255, data[i] + noise))
            data[i + 1] = max(0, min(255, data[i + 1] + noise))
            data[i + 2] = max(0, min(255, data[i + 2] + noise))

    def save(self, name):
        def chunk(kind, data):
            return struct.pack(">I", len(data)) + kind + data + struct.pack(
                ">I", zlib.crc32(kind + data) & 0xFFFFFFFF
            )
        scanlines = bytearray()
        stride = WIDTH * 3
        for y in range(HEIGHT):
            scanlines.append(0)
            scanlines.extend(self.pixels[y * stride:(y + 1) * stride])
        png = b"\x89PNG\r\n\x1a\n"
        png += chunk(b"IHDR", struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 2, 0, 0, 0))
        png += chunk(b"tEXt", b"Copyright\x00" + COPYRIGHT)
        png += chunk(b"IDAT", zlib.compress(scanlines, 9))
        png += chunk(b"IEND", b"")
        path = OUTPUT_DIR / name
        path.write_bytes(png)
        print(f"{path}: {WIDTH}x{HEIGHT} RGB PNG ({len(png)} bytes)")


def ridge(seed, base, amplitude, left=0, right=WIDTH, count=48):
    rng = random.Random(seed)
    points = []
    for i in range(count + 1):
        t = i / count
        height = amplitude * (
            0.46 + 0.26 * math.sin(t * 11 + 1.7)
            + 0.17 * math.sin(t * 27 + 0.4) + rng.uniform(-0.10, 0.10)
        )
        points.append((left + (right - left) * t, base - height))
    return points


def stars(canvas, rng, count, limit, color):
    for _ in range(count):
        x, y = rng.randrange(20, WIDTH - 20), rng.randrange(12, limit)
        alpha = rng.uniform(0.20, 0.75) * (1 - y / (limit * 1.7))
        canvas.blend(x, y, color, alpha)
        if rng.random() < 0.10:
            canvas.blend(x - 1, y, color, alpha * 0.4)
            canvas.blend(x + 1, y, color, alpha * 0.4)
            canvas.blend(x, y - 1, color, alpha * 0.4)
            canvas.blend(x, y + 1, color, alpha * 0.4)


def water(canvas, rng, horizon, top_color, bottom_color, reflection_x, light):
    canvas.gradient([(horizon, top_color), (HEIGHT - 1, bottom_color)], horizon)
    canvas.glow(reflection_x, horizon + 108, 140, 208, light, 0.18)
    for _ in range(190):
        t = rng.random()
        y = horizon + 4 + t * (HEIGHT - horizon - 8)
        x = rng.uniform(-70, WIDTH)
        length = rng.uniform(13, 95) * (0.6 + t)
        color = mix(top_color, light, rng.uniform(0.18, 0.46))
        canvas.line(x, y, x + length, y + rng.uniform(-0.8, 0.8), color,
                    rng.uniform(0.10, 0.28))
    for y in range(horizon + 3, HEIGHT - 5, 5):
        t = (y - horizon) / (HEIGHT - horizon)
        center = reflection_x + 11 * math.sin(y / 22) + rng.uniform(-10, 10)
        half = (12 + 125 * t) * rng.uniform(0.65, 1.15)
        gap = rng.uniform(3, 12)
        alpha = 0.18 + (1 - t) * 0.31
        canvas.line(center - half, y, center - gap, y, light, alpha)
        canvas.line(center + gap, y, center + half, y + 0.4, light, alpha * 0.85)


def solstice():
    canvas, rng = Canvas(), random.Random(71023)
    canvas.gradient([(0, (80, 48, 86)), (220, (214, 100, 98)),
                     (405, (255, 183, 116)), (639, (61, 52, 75))])
    canvas.glow(613, 220, 300, 252, (255, 190, 114), 0.46)
    canvas.glow(613, 220, 139, 139, (255, 225, 139), 0.22)
    canvas.disc(613, 220, 83, (255, 242, 183), (255, 193, 117))
    stars(canvas, rng, 63, 270, (255, 212, 160))
    water(canvas, rng, 404, (167, 114, 110), (31, 51, 71), 613, (255, 195, 129))
    canvas.polygon(ridge(11, 407, 88) + [(WIDTH, 410), (0, 410)],
                   (143, 108, 123), (118, 95, 112))
    canvas.polygon([(0, 310), (69, 344), (130, 319), (217, 371), (270, 353),
                    (353, 401), (423, 419), (0, 438)], (89, 81, 107), (63, 69, 91))
    canvas.polygon([(713, 415), (767, 363), (824, 377), (902, 326),
                    (960, 345), (960, 464)], (95, 82, 106), (62, 67, 89))
    canvas.polygon([(0, 442), (65, 464), (124, 474), (186, 513),
                    (218, 546), (286, 565), (341, 640), (0, 640)],
                   (37, 49, 66), (22, 37, 53))
    canvas.polygon([(848, 536), (899, 505), (960, 489), (960, 640),
                    (799, 640), (831, 582)], (30, 47, 62), (18, 34, 48))
    # Three understated birds, each drawn as two small tapered wings.
    for x, y, span in [(353, 233, 9), (379, 249, 6), (333, 255, 5)]:
        canvas.line(x - span, y - 2, x, y + 1, (100, 70, 90), 0.65)
        canvas.line(x, y + 1, x + span, y - 3, (100, 70, 90), 0.65)
    canvas.grain(71024)
    canvas.save("solstice.png")


def ring(canvas, cx, cy, rx, ry, angle, color, alpha, start=0, end=360):
    tilt = math.radians(angle)
    previous = None
    for degrees in range(start, end + 1, 2):
        theta = math.radians(degrees)
        x, y = rx * math.cos(theta), ry * math.sin(theta)
        point = (cx + x * math.cos(tilt) - y * math.sin(tilt),
                 cy + x * math.sin(tilt) + y * math.cos(tilt))
        if previous is not None:
            canvas.line(*previous, *point, color, alpha)
        previous = point


def afterglow():
    canvas, rng = Canvas(), random.Random(92051)
    canvas.gradient([(0, (16, 23, 55)), (220, (52, 39, 102)),
                     (431, (157, 81, 147)), (639, (19, 30, 63))])
    canvas.glow(602, 202, 251, 226, (155, 104, 238), 0.32)
    canvas.glow(390, 414, 590, 125, (244, 107, 178), 0.20)
    stars(canvas, rng, 185, 389, (207, 218, 255))
    ring(canvas, 602, 203, 191, 43, -18, (167, 177, 255), 0.38)
    ring(canvas, 602, 203, 184, 40, -18, (227, 166, 234), 0.56)
    canvas.disc(602, 203, 105, (186, 186, 255), (92, 75, 178), 0.055)
    canvas.glow(568, 171, 87, 81, (222, 207, 255), 0.18)
    ring(canvas, 602, 203, 191, 43, -18, (192, 180, 255), 0.75, 0, 180)
    ring(canvas, 602, 203, 184, 40, -18, (253, 174, 227), 0.58, 0, 180)
    water(canvas, rng, 453, (71, 60, 113), (16, 29, 57), 600, (180, 119, 218))
    canvas.polygon(ridge(31, 448, 84) + [(960, 453), (0, 453)],
                   (73, 58, 118), (60, 49, 102))
    canvas.polygon([(0, 353), (76, 391), (128, 365), (224, 416),
                    (271, 402), (363, 451), (0, 464)], (33, 40, 78))
    canvas.polygon([(724, 450), (785, 394), (836, 412), (910, 342),
                    (960, 366), (960, 481)], (35, 40, 76), (22, 33, 65))
    # A quiet, original low-rise neon skyline; no labels or lettering.
    for x in range(284, 713, 20):
        height, width = rng.randrange(17, 63), rng.randrange(10, 18)
        y = 456 - height
        canvas.rectangle(x, y, x + width, 459, (31, 35, 68))
        if height > 46:
            canvas.line(x + width / 2, y - 8, x + width / 2, y, (122, 108, 178), 0.55)
        for wy in range(y + 9, 451, 9):
            if rng.random() < 0.61:
                color = (224, 124, 189) if rng.random() < 0.65 else (110, 191, 201)
                canvas.line(x + 3, wy, x + min(width - 3, 7), wy, color, 0.53)
    canvas.line(266, 457, 722, 457, (234, 121, 180), 0.45)
    canvas.polygon([(0, 555), (52, 540), (119, 567), (197, 587),
                    (244, 640), (0, 640)], (17, 27, 54))
    canvas.polygon([(882, 567), (960, 528), (960, 640), (825, 640)], (15, 26, 53))
    canvas.grain(92052)
    canvas.save("afterglow.png")


def tidal():
    canvas, rng = Canvas(), random.Random(33071)
    canvas.gradient([(0, (8, 27, 56)), (210, (16, 67, 91)),
                     (385, (42, 125, 134)), (639, (7, 31, 58))])
    canvas.glow(371, 168, 232, 220, (97, 194, 204), 0.27)
    stars(canvas, rng, 155, 341, (181, 224, 230))
    ring(canvas, 371, 168, 114, 114, 0, (124, 207, 212), 0.16)
    canvas.glow(371, 168, 81, 81, (172, 229, 220), 0.38)
    canvas.disc(371, 168, 54, (217, 245, 228), (142, 206, 201))
    # Very gentle lunar surface shading, confined to the existing disc.
    canvas.glow(383, 183, 27, 19, (117, 178, 182), 0.20)
    canvas.glow(358, 157, 12, 10, (135, 192, 192), 0.15)
    water(canvas, rng, 383, (25, 99, 116), (5, 30, 57), 371, (116, 219, 205))
    canvas.polygon(ridge(57, 385, 50) + [(960, 388), (0, 388)],
                   (31, 89, 106), (23, 76, 96))
    canvas.polygon([(619, 388), (697, 339), (738, 348), (792, 314),
                    (825, 329), (892, 350), (960, 331), (960, 413)],
                   (15, 60, 82), (11, 49, 72))
    canvas.polygon([(0, 352), (53, 362), (99, 346), (142, 378),
                    (228, 396), (0, 416)], (16, 63, 82))
    # Long, gently bending tidal bands deepen toward the foreground.
    for index in range(18):
        y = 415 + (index / 17) ** 1.25 * 218
        amplitude = 2 + index * 0.43
        previous = None
        for x in range(-20, WIDTH + 21, 12):
            point = (x, y + amplitude * math.sin(x / (81 + index * 3) + index * 0.8))
            if previous is not None:
                canvas.line(*previous, *point, (73, 168, 172), 0.11 + index * 0.004)
            previous = point
    canvas.polygon([(0, 571), (58, 559), (105, 586), (153, 598),
                    (188, 640), (0, 640)], (6, 31, 52))
    canvas.polygon([(846, 598), (886, 572), (924, 574), (960, 555),
                    (960, 640), (816, 640)], (5, 28, 49))
    canvas.grain(33072)
    canvas.save("tidal.png")


if __name__ == "__main__":
    # Outputs only the three named PNG files, always beside this script.
    solstice()
    afterglow()
    tidal()
