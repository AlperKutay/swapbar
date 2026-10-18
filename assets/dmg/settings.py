# dmgbuild settings for Swapbar.dmg, used by scripts/dmg.sh.
# Icon positions must match the slots drawn in background.svg.
# scripts/dmg.sh passes absolute paths with -D app=... -D assets=...
import os.path

app = defines["app"]  # noqa: F821
assets = defines["assets"]  # noqa: F821

format = "UDZO"
compression_level = 9
filesystem = "HFS+"
size = None

files = [app]
symlinks = {"Applications": "/Applications"}
icon = os.path.join(assets, "AppIcon.icns")

background = os.path.join(assets, "dmg", "background.tiff")
window_rect = ((200, 160), (660, 420))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False

icon_size = 112
text_size = 13
arrange_by = None
icon_locations = {
    os.path.basename(app): (170, 236),
    "Applications": (490, 236),
}
# No hide_extensions: it sets com.apple.FinderInfo on the signed bundle, which fails
# `codesign --verify --strict`. Finder hides ".app" by default anyway.
