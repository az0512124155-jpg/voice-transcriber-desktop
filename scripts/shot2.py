"""Force the Voice Transcriber window to the front and grab it from screen.

    python shot2.py out.png
    python shot2.py out.png --size 1100 950
    python shot2.py out.png --click 300 200
"""
import ctypes
import sys
import time
from ctypes import wintypes

from PIL import ImageGrab

user32 = ctypes.windll.user32

# Without this the process is DPI-unaware, GetWindowRect returns scaled
# coordinates and the grab only covers the top-left corner of the window.
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # PER_MONITOR_AWARE_V2
except Exception:
    try:
        user32.SetProcessDPIAware()
    except Exception:
        pass

SW_RESTORE = 9
HWND_TOPMOST = -1
HWND_NOTOPMOST = -2
SWP_NOMOVE = 0x0002
SWP_NOSIZE = 0x0001
SWP_SHOWWINDOW = 0x0040


def find():
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
        if buf.value != "Voice Transcriber":
            return True
        rect = wintypes.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(rect))
        if rect.right - rect.left > 300:
            found.append((hwnd, rect))
        return True

    user32.EnumWindows(callback, 0)
    if not found:
        raise SystemExit("Voice Transcriber window not found")
    return found[0]


def front(hwnd):
    user32.ShowWindow(hwnd, SW_RESTORE)
    user32.SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW)
    user32.BringWindowToTop(hwnd)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.9)


def click(hwnd, rect, x, y):
    front(hwnd)
    user32.SetCursorPos(rect.left + x, rect.top + y)
    time.sleep(0.2)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.06)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(1.4)


hwnd, rect = find()
out = sys.argv[1]
args = sys.argv[2:]
if "--size" in args:
    i = args.index("--size")
    user32.MoveWindow(hwnd, rect.left, rect.top, int(args[i + 1]),
                      int(args[i + 2]), True)
    time.sleep(1.2)
    hwnd, rect = find()
if "--click" in args:
    i = args.index("--click")
    click(hwnd, rect, int(args[i + 1]), int(args[i + 2]))
    hwnd, rect = find()

front(hwnd)
img = ImageGrab.grab(
    bbox=(rect.left, rect.top, rect.right, rect.bottom), all_screens=True
)
img.save(out)
user32.SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE)
print("saved", out, img.size)
