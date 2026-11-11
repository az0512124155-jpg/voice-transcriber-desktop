"""Capture the largest top-level window whose title contains the given text.

    python shot.py "Voice Transcriber" out.png

Uses PrintWindow with PW_RENDERFULLCONTENT so a WebView2 surface is included.
"""
import ctypes
import sys
from ctypes import wintypes

from PIL import Image

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32

# Without this the process sees DPI-virtualised coordinates: on a 200% display
# GetWindowRect reports half the real size and PrintWindow hands back a crop of
# the top-left quadrant of the surface.
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # per-monitor aware
except Exception:
    user32.SetProcessDPIAware()

PW_RENDERFULLCONTENT = 0x00000002


def windows():
    found = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def callback(hwnd, _):
        if not user32.IsWindowVisible(hwnd):
            return True
        length = user32.GetWindowTextLengthW(hwnd)
        if length == 0:
            return True
        buf = ctypes.create_unicode_buffer(length + 1)
        user32.GetWindowTextW(hwnd, buf, length + 1)
        rect = wintypes.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(rect))
        w = rect.right - rect.left
        h = rect.bottom - rect.top
        if w > 0 and h > 0:
            found.append((buf.value, hwnd, w, h))
        return True

    user32.EnumWindows(callback, 0)
    return found


title = sys.argv[1]
out = sys.argv[2]
matches = [w for w in windows() if title.lower() in w[0].lower()]
if not matches:
    print("no window matching", title)
    for w in windows():
        print("  ", w[0], w[2], "x", w[3])
    sys.exit(1)

matches.sort(key=lambda w: w[2] * w[3], reverse=True)
name, hwnd, width, height = matches[0]
print("capturing %r %dx%d" % (name, width, height))

hdc = user32.GetWindowDC(hwnd)
mem_dc = gdi32.CreateCompatibleDC(hdc)
bitmap = gdi32.CreateCompatibleBitmap(hdc, width, height)
gdi32.SelectObject(mem_dc, bitmap)
user32.PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT)

class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


info = BITMAPINFOHEADER()
info.biSize = ctypes.sizeof(BITMAPINFOHEADER)
info.biWidth = width
info.biHeight = -height           # top-down rows
info.biPlanes = 1
info.biBitCount = 32
info.biCompression = 0

buffer = ctypes.create_string_buffer(width * height * 4)
gdi32.GetDIBits(mem_dc, bitmap, 0, height, buffer, ctypes.byref(info), 0)
image = Image.frombuffer("RGBA", (width, height), buffer, "raw", "BGRA", 0, 1)
image.convert("RGB").save(out)
print("saved", out)

gdi32.DeleteObject(bitmap)
gdi32.DeleteDC(mem_dc)
user32.ReleaseDC(hwnd, hdc)
