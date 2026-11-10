"""Restore the Voice Transcriber window, drive it, and capture one frame.

    python win.py restore
    python win.py size W H
    python win.py click X Y
    python win.py scroll X Y N     (N wheel notches, negative = down)
    python win.py shot out.png
"""
import ctypes
import sys
import time
from ctypes import wintypes

from PIL import ImageGrab

try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    pass

user32 = ctypes.windll.user32
HWND_TOPMOST = -1
HWND_NOTOPMOST = -2
SWP_NOSIZE = 0x0001
SWP_NOMOVE = 0x0002
SWP_SHOWWINDOW = 0x0040


def find():
    found = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def cb(hwnd, _):
        n = user32.GetWindowTextLengthW(hwnd)
        if n == 0:
            return True
        buf = ctypes.create_unicode_buffer(n + 1)
        user32.GetWindowTextW(hwnd, buf, n + 1)
        if buf.value != "Voice Transcriber":
            return True
        r = wintypes.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(r))
        found.append((hwnd, r))
        return True

    user32.EnumWindows(cb, 0)
    if not found:
        raise SystemExit("no Voice Transcriber window")
    # The main window is the one that is not the tiny tray window.
    found.sort(key=lambda f: (f[1].right - f[1].left) * (f[1].bottom - f[1].top),
               reverse=True)
    return found[0]


def front(hwnd):
    user32.ShowWindow(hwnd, 9)  # SW_RESTORE
    user32.SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW)
    user32.BringWindowToTop(hwnd)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.8)


cmd = sys.argv[1]
hwnd, rect = find()
front(hwnd)
hwnd, rect = find()

if cmd == "restore":
    print("restored", rect.right - rect.left, rect.bottom - rect.top,
          "at", rect.left, rect.top)
elif cmd == "size":
    w, h = int(sys.argv[2]), int(sys.argv[3])
    user32.MoveWindow(hwnd, rect.left, rect.top, w, h, True)
    time.sleep(1.0)
    _, r2 = find()
    print("resized to", r2.right - r2.left, r2.bottom - r2.top)
elif cmd == "click":
    x, y = int(sys.argv[2]), int(sys.argv[3])
    user32.SetCursorPos(rect.left + x, rect.top + y)
    time.sleep(0.2)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.06)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(1.3)
    print("clicked", x, y)
elif cmd == "scroll":
    x, y, n = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
    user32.SetCursorPos(rect.left + x, rect.top + y)
    time.sleep(0.2)
    for _ in range(abs(n)):
        user32.mouse_event(0x0800, 0, 0, -120 if n > 0 else 120, 0)
        time.sleep(0.12)
    time.sleep(0.9)
    print("scrolled", n)
elif cmd == "shot":
    user32.SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE)
    time.sleep(0.5)
    img = ImageGrab.grab(
        bbox=(rect.left, rect.top, rect.right, rect.bottom), all_screens=True
    )
    img.save(sys.argv[2])
    print("saved", sys.argv[2], img.size)
    user32.SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE)
