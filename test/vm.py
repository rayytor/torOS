#!/usr/bin/env python3
"""Drive the QEMU test VM by hand: screenshots and mouse clicks over QMP.
  test/vm.py start [nozapret] boot out/toros.img in the background (changes discarded)
        [disk2=FILE]          with FILE as a second, empty disk that keeps what is written to
                              it (for trying toros-install; make it with: truncate -s 8G FILE)
        [boot=FILE]           boot FILE instead of out/toros.img (a disk installed that way)
  test/vm.py shot NAME        save out/vm-NAME.png
  test/vm.py film NAME [SECS] watch the screen for SECS seconds (default 15; run it right after
                              start to see the boot): prints what is on the screen and when
                              (black, the boot logo with its bar, the desktop, or something
                              else on a black screen: firmware pictures, console text), saves
                              one picture per change as out/vm-NAME-TTT.png (TTT = 1/100 s),
                              and says whether anything showed between the logo and the desktop
  test/vm.py click X Y [btn]  click at pixel X,Y of the 1280x800 guest screen (btn: left|right)
  test/vm.py move X Y         move the pointer there without clicking (hover states)
  test/vm.py drag X1 Y1 X2 Y2 [X3 Y3 ...] [hold]
                              press at X1,Y1, move in steps to X2,Y2 (and on to further
                              points) and let go (with "hold": keep the button down, to look
                              at what a drag shows; "test/vm.py drag X Y X Y" lets go again)
  test/vm.py hold X Y SECS    press the button at X,Y, keep it down for SECS seconds, let go
                              (the session menu's rows that have to be held)
  test/vm.py key KEYS         press a key combo, e.g. meta_l-ret
  test/vm.py keydown KEY      press a key and keep it down (KEY as QEMU names it: meta_l, spc)
  test/vm.py keyup KEY        let it go again (to hold Super for a while before another key)
  test/vm.py type "TEXT"      type text (letters, digits, space, - . / _), then Enter
  test/vm.py running          exit 0 if the VM is still running
  test/vm.py stop
The VM's SSH port is forwarded to 127.0.0.1:2222, so push.sh can be tried on it:
  TOROS_HOST=127.0.0.1:2222 ./push.sh
"""
import json, os, shutil, socket, subprocess, sys, time

os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SOCK, W, H = "out/vm-qmp.sock", 1280, 800

def qmp(*cmds):
    s = socket.socket(socket.AF_UNIX); s.connect(SOCK); f = s.makefile("rw")
    f.readline(); out = []
    for c in ({"execute": "qmp_capabilities"},) + cmds:
        f.write(json.dumps(c) + "\n"); f.flush()
        while True:
            r = json.loads(f.readline())
            if "return" in r or "error" in r: out.append(r); break
    return out[1:]

def qmp_wait():
    for _ in range(100):
        try: qmp({"execute": "query-status"}); return
        except OSError: time.sleep(0.05)
    sys.exit("the VM does not answer")

def look(im):
    """What a screenshot shows: 'black', 'logo' (the boot logo on black, with the
    splash's bar as 'logo NN%' once it is drawn), 'desktop' (a screen that is not
    mostly black) or 'other' (anything else on a black screen)."""
    from PIL import Image, ImageChops
    with open("kernel/logo.ppm") as f:
        f.readline(); f.readline(); lw, lh = map(int, f.readline().split())
    w, h = im.size
    logo = ((w - lw) // 2, (h - lh) // 2, (w + lw) // 2, (h + lh) // 2)
    bar = ((w - 160) // 2, (h + lh) // 2 + 44, (w + 160) // 2, (h + lh) // 2 + 48)
    rest = im.copy(); rest.paste((0, 0, 0), logo); rest.paste((0, 0, 0), bar)
    lit = rest.convert("L").point(lambda v: 255 if v > 24 else 0)
    if lit.getbbox(): return "desktop" if lit.histogram()[255] > w * h // 5 else "other"
    if not im.crop(logo).point(lambda v: 255 if v > 24 else 0).getbbox(): return "black"
    if ImageChops.difference(im.crop(logo), Image.open("kernel/logo.ppm").convert("RGB")).getbbox(): return "other"
    row = im.crop((bar[0], bar[1] + 1, bar[2], bar[1] + 2)).convert("RGB")
    px = [row.getpixel((x, 0)) for x in range(160)]
    if max(max(p) for p in px) <= 24: return "logo"
    return "logo %d%%" % (sum(1 for r, g, b in px if g > 100 and g > r + 40) * 100 // 160)

def film(name, secs):
    from PIL import Image
    qmp_wait()
    t0, last, seen, kinds = time.time(), None, None, []
    while time.time() - t0 < secs:
        t = time.time() - t0
        qmp({"execute": "screendump", "arguments": {"filename": "out/vm-film.ppm"}})
        try: im = Image.open("out/vm-film.ppm").convert("RGB"); im.load()
        except Exception: continue
        data = im.tobytes()
        if data != last:
            kind = look(im)
            # the bar moves all the time: one line and one picture per kind of screen
            if seen is None or kind.split()[0] != seen.split()[0] or kind == "other":
                print(f"{t:6.2f} s  {kind}"); im.save(f"out/vm-{name}-{int(t * 100):04d}.png")
                kinds.append(kind.split()[0])
            seen, last = kind, data
        time.sleep(0.03)
    print(f"{time.time() - t0:6.2f} s  end ({seen})")
    os.remove("out/vm-film.ppm")
    # from the first logo on there may only be the logo, black and then the desktop
    if "logo" not in kinds or "desktop" not in kinds:
        print("boot film: PROBLEM, no boot logo seen" if "logo" not in kinds else "boot film: PROBLEM, no desktop seen")
    elif "other" in kinds[kinds.index("logo"):]:
        print("boot film: PROBLEM, something else was on the screen after the logo (console text?)")
    else:
        print("boot film: ok, only the logo and black between the firmware and the desktop")

def main():
    cmd = sys.argv[1]
    if cmd == "start":
        shutil.copy("/usr/share/OVMF/OVMF_VARS_4M.fd", "out/OVMF_VARS.fd")
        opt = dict(a.split("=", 1) for a in sys.argv[2:] if "=" in a)
        if os.path.exists(SOCK): os.remove(SOCK)
        subprocess.Popen(["qemu-system-x86_64", "-enable-kvm", "-cpu", "host", "-smp", "2", "-m", "4096",
            "-machine", "q35", "-display", "none",
            "-drive", "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd",
            "-drive", "if=pflash,format=raw,file=out/OVMF_VARS.fd",
            "-drive", f"file={opt.get('boot', 'out/toros.img')},format=raw,if=virtio,snapshot=on",
            *(["-drive", f"file={opt['disk2']},format=raw,if=virtio"] if "disk2" in opt else []),
            "-device", "virtio-vga", "-device", "qemu-xhci", "-device", "usb-tablet",
            "-audiodev", "none,id=a0", "-device", "intel-hda", "-device", "hda-duplex,audiodev=a0",
            "-netdev", "user,id=n0,hostfwd=tcp:127.0.0.1:2222-:22", "-device", "virtio-net-pci,netdev=n0",
            "-fw_cfg", "name=opt/toros/virtwifi,string=1",
            *(["-fw_cfg", "name=opt/toros/nozapret,string=1"] if "nozapret" in sys.argv else []),
            "-qmp", f"unix:{SOCK},server,nowait", "-serial", "null"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    elif cmd == "shot":
        ppm = f"out/vm-{sys.argv[2]}.ppm"
        qmp({"execute": "screendump", "arguments": {"filename": ppm}}); time.sleep(0.5)
        from PIL import Image
        Image.open(ppm).save(ppm[:-4] + ".png"); os.remove(ppm)
    elif cmd == "film":
        film(sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 15)
    elif cmd in ("click", "move"):
        x, y = int(sys.argv[2]), int(sys.argv[3])
        btn = sys.argv[4] if len(sys.argv) > 4 else "left"
        pos = [{"type": "abs", "data": {"axis": "x", "value": x * 32767 // W}},
               {"type": "abs", "data": {"axis": "y", "value": y * 32767 // H}}]
        if cmd == "move":
            qmp({"execute": "input-send-event", "arguments": {"events": pos}}); return
        ev = lambda down: {"execute": "input-send-event", "arguments": {"events":
              [{"type": "btn", "data": {"down": down, "button": btn}}]}}
        qmp({"execute": "input-send-event", "arguments": {"events": pos}}); time.sleep(0.3)
        qmp(ev(True)); time.sleep(0.1); qmp(ev(False))
    elif cmd == "drag":
        xy = [int(a) for a in sys.argv[2:] if a != "hold"]
        points = list(zip(xy[0::2], xy[1::2]))
        at = lambda x, y: {"execute": "input-send-event", "arguments": {"events": [
              {"type": "abs", "data": {"axis": "x", "value": x * 32767 // W}},
              {"type": "abs", "data": {"axis": "y", "value": y * 32767 // H}}]}}
        btn = lambda down: {"execute": "input-send-event", "arguments": {"events":
              [{"type": "btn", "data": {"down": down, "button": "left"}}]}}
        qmp(at(*points[0])); time.sleep(0.3); qmp(btn(True)); time.sleep(0.1)
        for (x1, y1), (x2, y2) in zip(points, points[1:]):
            for i in range(1, 13):
                qmp(at(x1 + (x2 - x1) * i // 12, y1 + (y2 - y1) * i // 12)); time.sleep(0.03)
        time.sleep(0.2)
        if "hold" not in sys.argv: qmp(btn(False))
    elif cmd == "hold":
        x, y = int(sys.argv[2]), int(sys.argv[3])
        btn = lambda down: {"execute": "input-send-event", "arguments": {"events":
              [{"type": "btn", "data": {"down": down, "button": "left"}}]}}
        qmp({"execute": "input-send-event", "arguments": {"events": [
              {"type": "abs", "data": {"axis": "x", "value": x * 32767 // W}},
              {"type": "abs", "data": {"axis": "y", "value": y * 32767 // H}}]}})
        time.sleep(0.3); qmp(btn(True)); time.sleep(float(sys.argv[4])); qmp(btn(False))
    elif cmd in ("keydown", "keyup"):
        qmp({"execute": "input-send-event", "arguments": {"events": [{"type": "key", "data":
              {"down": cmd == "keydown", "key": {"type": "qcode", "data": sys.argv[2]}}}]}})
    elif cmd == "key":
        qmp({"execute": "human-monitor-command", "arguments": {"command-line": "sendkey " + sys.argv[2]}})
    elif cmd == "type":
        names = {" ": "spc", "-": "minus", ".": "dot", "/": "slash", "_": "shift-minus"}
        for ch in sys.argv[2]:
            k = names.get(ch, ("shift-" + ch.lower()) if ch.isupper() else ch)
            qmp({"execute": "human-monitor-command", "arguments": {"command-line": "sendkey " + k}}); time.sleep(0.05)
        qmp({"execute": "human-monitor-command", "arguments": {"command-line": "sendkey ret"}})
    elif cmd == "running":
        try: qmp({"execute": "query-status"})
        except OSError: sys.exit(1)
    elif cmd == "stop":
        try: qmp({"execute": "quit"})
        except OSError: pass

main()
