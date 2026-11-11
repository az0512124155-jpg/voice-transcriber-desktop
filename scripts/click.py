"""Click inside a window by window-relative coordinates.

    python scripts/click.py "Voice Transcriber" 150 336

Coordinates are physical pixels relative to the window's top-left corner, so a
coordinate read off a screenshot taken by shot.py can be passed straight in.
"""
import ctypes
import sys
import time
from ctypes import wintypes

user32 = ctypes.windll.user32

try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    user32.SetProcessDPIAware()

MOUSEEVENTF_LEFTDOWN = 0x0002
MOUSEEVENTF_LEFTUP = 0x0004

title, dx, dy = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])

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
    found.append((buf.value, hwnd, rect))
    return True


user32.EnumWindows(callback, 0)
matches = [w for w in found if title.lower() in w[0].lower()]
if not matches:
    print("no window matching", title)
    sys.exit(1)

name, hwnd, rect = matches[0]
user32.ShowWindow(hwnd, 9)  # SW_RESTORE
user32.SetForegroundWindow(hwnd)
time.sleep(0.4)

x = rect.left + dx
y = rect.top + dy
user32.SetCursorPos(x, y)
time.sleep(0.15)
user32.mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
time.sleep(0.06)
user32.mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
print(f"clicked {name!r} at window+({dx},{dy}) -> screen ({x},{y})")
