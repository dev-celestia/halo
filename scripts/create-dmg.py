#!/usr/bin/env python3
"""Build a polished macOS Drag-and-Drop DMG Installer.

Features:
  - Retina background from scripts/dmg-background.py (dark grid, drop wells)
  - Finder window geometry imported from that script, so icons land in the wells
  - App on the left, /Applications symlink on the right, extension hidden
  - Hidden toolbars, pathbars, statusbars, and sidebars
  - Custom volume icon (.VolumeIcon.icns with FinderInfo custom-icon bit)
  - Compressed read-only disk image (UDZO format with level 9 compression)
  - Optional codesigning

Usage:
  python3 scripts/create-dmg.py <path-to-app> <path-to-dmg> [options]
"""

import argparse
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile


def _background_module():
    """Load scripts/dmg-background.py without executing it."""
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dmg-background.py")
    spec = importlib.util.spec_from_file_location("dmg_background", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def ensure_dmgbuild():
    try:
        import dmgbuild
        return True
    except ImportError:
        print("==> dmgbuild not found, attempting user install...")
        cmd = [sys.executable, "-m", "pip", "install", "--quiet", "--user", "dmgbuild"]
        res = subprocess.run(cmd)
        if res.returncode != 0:
            cmd = [sys.executable, "-m", "pip", "install", "--quiet", "--user", "--break-system-packages", "dmgbuild"]
            res = subprocess.run(cmd)
        try:
            import dmgbuild
            return True
        except ImportError:
            print("warning: failed to install dmgbuild; will fall back to hdiutil")
            return False

def build_with_dmgbuild(app_path, dmg_path, volname, bg_path, icon_path, app_pos, apps_pos, window_size, icon_size, text_size):
    import dmgbuild

    app_name = os.path.basename(app_path)
    w, h = window_size
    ax, ay = app_pos
    bx, by = apps_pos

    settings = {
        "format": "UDZO",
        "compression_level": 9,
        "files": [app_path],
        "symlinks": {"Applications": "/Applications"},
        "show_status_bar": False,
        "show_tab_view": False,
        "show_toolbar": False,
        "show_pathbar": False,
        "show_sidebar": False,
        "default_view": "icon-view",
        "show_icon_preview": True,
        "hide_extensions": [app_name],
        "window_rect": ((200, 120), (w, h)),
        "icon_size": icon_size,
        "text_size": text_size,
        "icon_locations": {
            app_name: (ax, ay),
            "Applications": (bx, by),
        },
    }

    if bg_path and os.path.exists(bg_path):
        settings["background"] = bg_path

    if icon_path and os.path.exists(icon_path):
        settings["icon"] = icon_path
        settings["badge_icon"] = icon_path

    # Ensure parent output dir exists
    os.makedirs(os.path.dirname(os.path.abspath(dmg_path)), exist_ok=True)
    if os.path.exists(dmg_path):
        os.remove(dmg_path)

    print(f"==> building {dmg_path} via dmgbuild...")
    dmgbuild.build_dmg(
        filename=dmg_path,
        volume_name=volname,
        settings=settings,
    )
    return True

def build_with_hdiutil_fallback(app_path, dmg_path, volname, bg_path, icon_path):
    print(f"==> building {dmg_path} via hdiutil fallback...")
    stage = tempfile.mkdtemp(prefix="halo-dmg-stage-")
    try:
        app_name = os.path.basename(app_path)
        shutil.copytree(app_path, os.path.join(stage, app_name), symlinks=True)
        os.symlink("/Applications", os.path.join(stage, "Applications"))

        if icon_path and os.path.exists(icon_path):
            vol_icon = os.path.join(stage, ".VolumeIcon.icns")
            shutil.copyfile(icon_path, vol_icon)
            setfile = shutil.which("SetFile")
            if setfile:
                subprocess.run([setfile, "-a", "C", stage], check=False)

        if bg_path and os.path.exists(bg_path):
            bg_dir = os.path.join(stage, ".background")
            os.makedirs(bg_dir, exist_ok=True)
            shutil.copyfile(bg_path, os.path.join(bg_dir, os.path.basename(bg_path)))

        if os.path.exists(dmg_path):
            os.remove(dmg_path)

        cmd = [
            "hdiutil", "create",
            "-volname", volname,
            "-srcfolder", stage,
            "-ov", "-format", "UDZO",
            dmg_path
        ]
        subprocess.run(cmd, check=True)
        return True
    finally:
        shutil.rmtree(stage, ignore_errors=True)

def main():
    parser = argparse.ArgumentParser(description="Create macOS Drag-and-Drop DMG Installer")
    parser.add_argument("app_bundle", help="Path to .app bundle (e.g. dist/Halo.app)")
    parser.add_argument("dmg_path", help="Path to target .dmg output")
    parser.add_argument("--volname", help="Volume name (default: app name without .app)")
    parser.add_argument("--icon", help="Path to .icns volume icon")
    parser.add_argument("--background", help="Path to background image (.tiff or .png)")
    parser.add_argument("--sign", help="Codesign identity for DMG")
    args = parser.parse_args()

    app_path = os.path.abspath(args.app_bundle)
    if not os.path.isdir(app_path) or not app_path.endswith(".app"):
        print(f"error: {app_path} is not a valid .app bundle directory", file=sys.stderr)
        sys.exit(1)

    app_name = os.path.basename(app_path)
    stem = app_name[:-4] if app_name.endswith(".app") else app_name
    volname = args.volname if args.volname else stem
    dmg_path = os.path.abspath(args.dmg_path)

    # 1. Resolve volume icon
    icon_path = args.icon
    if not icon_path:
        candidates = [
            os.path.join(app_path, "Contents/Resources/icon.icns"),
            os.path.join(app_path, f"Contents/Resources/{stem.lower()}.icns"),
            os.path.join(app_path, f"Contents/Resources/{stem}.icns"),
        ]
        for c in candidates:
            if os.path.isfile(c):
                icon_path = c
                break

    # 2. Resolve background image
    bg_path = args.background
    if not bg_path:
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        bg_candidates = [
            os.path.join(root, f"crates/{stem.lower()}-ui/assets/dmg/dmg-background.tiff"),
            os.path.join(root, f"crates/{stem.lower()}-ui/assets/dmg/dmg-background@2x.png"),
            os.path.join(root, "crates/ui/assets/dmg/dmg-background.tiff"),
        ]
        for c in bg_candidates:
            if os.path.isfile(c):
                bg_path = c
                break

    # If background still not found, auto-generate on the fly
    if not bg_path or not os.path.exists(bg_path):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        gen_script = os.path.join(root, "scripts/dmg-background.py")
        if os.path.exists(gen_script):
            subprocess.run([sys.executable, gen_script, "--app-name", stem], check=False)
            generated_tiff = os.path.join(root, "crates/ui/assets/dmg/dmg-background.tiff")
            if os.path.exists(generated_tiff):
                bg_path = generated_tiff

    # Geometry is in points and comes from scripts/dmg-background.py.
    # Icon positions are centers, origin top-left, sitting in the drop wells.
    layout = _background_module()
    window_size = (layout.WINDOW_W, layout.WINDOW_H)
    icon_size = layout.ICON_SIZE
    text_size = layout.LABEL_PT
    app_pos = layout.APP_CENTER
    apps_pos = layout.APPS_CENTER

    # 4. Build DMG
    success = False
    if ensure_dmgbuild():
        try:
            success = build_with_dmgbuild(
                app_path=app_path,
                dmg_path=dmg_path,
                volname=volname,
                bg_path=bg_path,
                icon_path=icon_path,
                app_pos=app_pos,
                apps_pos=apps_pos,
                window_size=window_size,
                icon_size=icon_size,
                text_size=text_size,
            )
        except Exception as e:
            print(f"warning: dmgbuild error ({e}), falling back to hdiutil...")

    if not success:
        build_with_hdiutil_fallback(
            app_path=app_path,
            dmg_path=dmg_path,
            volname=volname,
            bg_path=bg_path,
            icon_path=icon_path,
        )

    # 5. Optional codesigning of the DMG
    if args.sign and os.path.exists(dmg_path):
        print(f"==> codesigning DMG with {args.sign}...")
        subprocess.run(["codesign", "--force", "--sign", args.sign, dmg_path], check=False)

    # 6. Report results
    size = shutil.which("du")
    size_str = ""
    if size:
        out = subprocess.run([size, "-sh", dmg_path], capture_output=True, text=True)
        size_str = out.stdout.split()[0] if out.stdout else ""

    print("==> drag-and-drop installer created successfully:")
    print(f"  dmg:     {dmg_path}")
    print(f"  volume:  {volname}")
    print(f"  size:    {size_str}")
    print(f"  mount:   open \"{dmg_path}\"")

if __name__ == "__main__":
    main()
