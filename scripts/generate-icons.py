#!/usr/bin/env python3
import sys
import numpy as np
from PIL import Image, ImageFilter
from collections import deque
import os

src_path = "/home/ritiksaini/.gemini/antigravity-ide/brain/3be039b7-7c2c-4094-b1e7-38a8b8d41b32/personal_notepad_master_icon_1792217275895.jpg"
out_master = "assets/app-icon.png"

im = Image.open(src_path).convert("RGB")
w, h = im.size
arr = np.array(im, dtype=np.uint8)

# BFS to identify outer background without touching white pages inside
visited = np.zeros((h, w), dtype=bool)
q = deque()

for x in range(w):
    q.append((0, x))
    q.append((h - 1, x))
for y in range(h):
    q.append((y, 0))
    q.append((y, w - 1))

while q:
    y, x = q.popleft()
    if visited[y, x]:
        continue
    visited[y, x] = True
    r, g, b = int(arr[y, x, 0]), int(arr[y, x, 1]), int(arr[y, x, 2])
    # Outside background brightness threshold
    if min(r, g, b) > 218:
        for dy, dx in [(-1,0), (1,0), (0,-1), (0,1)]:
            ny, nx = y + dy, x + dx
            if 0 <= ny < h and 0 <= nx < w and not visited[ny, nx]:
                nr, ng, nb = int(arr[ny, nx, 0]), int(arr[ny, nx, 1]), int(arr[ny, nx, 2])
                if min(nr, ng, nb) > 218:
                    q.append((ny, nx))

# Create initial alpha mask: foreground is 255, background is 0
alpha_arr = np.where(visited, 0, 255).astype(np.uint8)
mask = Image.fromarray(alpha_arr, mode="L")

# Apply slight Gaussian blur to anti-alias edges
smoothed_mask = mask.filter(ImageFilter.GaussianBlur(radius=1.2))

# Combine RGB with smoothed alpha
rgba = im.convert("RGBA")
rgba.putalpha(smoothed_mask)

os.makedirs("assets", exist_ok=True)
rgba.save(out_master, "PNG")
print(f"Master icon saved to {out_master} ({w}x{h})")
