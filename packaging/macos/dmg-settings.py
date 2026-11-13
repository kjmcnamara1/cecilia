# dmgbuild settings for the Cecilia .dmg (see bundle.sh). dmgbuild runs this file with a
# `defines` dict from its -D options: app (Cecilia.app), background (a .tiff or .png) and icon
# (the volume icon, an .icns). The window is laid out to match dmg-background.svg: 640 x 400
# points, the app on the left and the Applications link on the right of the arrow.

# ruff: noqa: F821  (`defines` is supplied by dmgbuild)
app = defines["app"]

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines["icon"]
background = defines["background"]

window_rect = ((200, 120), (640, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False

arrange_by = None
icon_size = 128
text_size = 13
icon_locations = {
    "Cecilia.app": (170, 185),
    "Applications": (470, 185),
}
