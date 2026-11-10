"""Drive the running Voice Transcriber window: maximize, click, capture.

    python drive.py max
    python drive.py click 150 224
    python drive.py shot out.png
"""
import ctypes
import sys
import time
from ctypes import wintypes

from PIL import Image

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
PW_RENDERFULLCONTENT = 0x00000002


def find(title):
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
        w, h = rect.right - rect.left, rect.bottom - rect.top
        if w > 0 and h > 0:
            found.append((buf.value, hwnd, rect))
        return True

    user32.EnumWindows(callback, 0)
    matches = [f for f in found if title.lower() in f[0].lower()]
    matches.sort(key=lambda f: (f[2].right - f[2].left) * (f[2].bottom - f[2].top), reverse=True)
    return matches[0]


def capture(hwnd, rect, out):
    w, h = rect.right - rect.left, rect.bottom - rect.top
    hdc = user32.GetWindowDC(hwnd)
    mem = gdi32.CreateCompatibleDC(hdc)
    bmp = gdi32.CreateCompatibleBitmap(hdc, w, h)
    gdi32.SelectObject(mem, bmp)
    user32.PrintWindow(hwnd, mem, PW_RENDERFULLCONTENT)

    class BIH(ctypes.Structure):
        _fields_ = [
            ("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG),
            ("biHeight", wintypes.LONG), ("biPlanes", wintypes.WORD),
            ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
            ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG),
            ("biYPelsPerMeter", wintypes.LONG), ("biClrUsed", wintypes.DWORD),
            ("biClrImportant", wintypes.DWORD),
        ]

    info = BIH()
    info.biSize = ctypes.sizeof(BIH)
    info.biWidth, info.biHeight = w, -h
    info.biPlanes, info.biBitCount, info.biCompression = 1, 32, 0
    buf = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(mem, bmp, 0, h, buf, ctypes.byref(info), 0)
    Image.frombuffer("RGBA", (w, h), buf, "raw", "BGRA", 0, 1).convert("RGB").save(out)
    gdi32.DeleteObject(bmp)
    gdi32.DeleteDC(mem)
    user32.ReleaseDC(hwnd, hdc)
    print("saved", out, w, "x", h)


name, hwnd, rect = find("Voice Transcriber")
cmd = sys.argv[1]

if cmd == "max":
    user32.ShowWindow(hwnd, 3)          # SW_MAXIMIZE
    user32.SetForegroundWindow(hwnd)
    time.sleep(1.5)
    _, hwnd, rect = find("Voice Transcriber")
    print("maximized to", rect.right - rect.left, "x", rect.bottom - rect.top)
elif cmd == "size":
    w, h = int(sys.argv[2]), int(sys.argv[3])
    user32.ShowWindow(hwnd, 9)          # SW_RESTORE
    user32.MoveWindow(hwnd, 60, 60, w, h, True)
    time.sleep(1.5)
    _, hwnd, rect = find("Voice Transcriber")
    print("resized to", rect.right - rect.left, "x", rect.bottom - rect.top,
          "at", rect.left, rect.top)
elif cmd == "click":
    x, y = int(sys.argv[2]), int(sys.argv[3])
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.4)
    user32.SetCursorPos(rect.left + x, rect.top + y)
    time.sleep(0.15)
    user32.mouse_event(0x0002, 0, 0, 0, 0)   # left down
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)   # left up
    time.sleep(1.2)
    print("clicked", x, y)
elif cmd == "shot":
    capture(hwnd, rect, sys.argv[2])
