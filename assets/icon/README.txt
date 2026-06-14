SHIORI 栞 — APP ICON SET
========================

Design: Shippori Mincho 栞 with a vermilion bookmark tucked behind the glyph
(Mid position, Long, Bold). Two grounds — washi (light, primary) and sumi (dark).
All PNGs are rendered crisp from the live glyph; square app-icon masters are fully
opaque (no alpha), as the App Store requires.


FOLDER STRUCTURE
----------------
source/      shiori-1024-light.png, shiori-1024-dark.png
             → full-bleed 1024×1024 masters. Start here for anything custom.

ios/         AppIcon.appiconset/      → drag into Xcode's asset catalog as-is.
                                         Contains every iPhone/iPad size + Contents.json.
             AppIcon-Dark.appiconset/ → dark 1024 master for an alternate app icon.

macos/       Shiori.icns              → ready-to-use macOS app icon (double-click to preview).
             Shiori-1024.png          → source if you need to rebuild.

android/     mipmap-mdpi … xxxhdpi/   → ic_launcher.png (square) + ic_launcher_round.png (circle)
             playstore-icon-512.png   → Google Play listing icon.

web/         favicon.ico              → multi-size (16/32/48) classic favicon.
             favicon-16/32/48.png     → modern PNG favicons.
             apple-touch-icon-180.png → iOS home-screen / Safari.
             android-chrome-192/512.png, maskable-512.png
             site.webmanifest         → PWA manifest (edit name/paths as needed).

desktop/     shiori.ico               → multi-size (16–256) icon embedded in the Windows .exe.
             shiori-64.rgba           → raw RGBA for the eframe window icon (no runtime decoder).
             → derived from display/rounded-1024-light.png; consumed by crates/shiori-gui.

display/     rounded-* and circle-* in light + dark.
             → for marketing, social avatars, decks, docs (NOT for the App Store —
               iOS applies its own corner mask to the square master).


QUICK USE
---------
iOS    : Drag ios/AppIcon.appiconset into your Xcode project's Assets.xcassets.
macOS  : Use macos/Shiori.icns directly, or set it in your target's settings.
Android: Copy the mipmap-* folders into app/src/main/res/.
Web    : Copy web/* to your site root and add the tags in web-snippet.html to <head>.
Windows: desktop/* are already wired into crates/shiori-gui (build.rs + main.rs) — no action needed.


COLORS
------
washi (light bg)   #f3ebd7 → #e6d9bc
sumi  (dark bg)    #1c1c22 → #121216
vermilion marker   #9a2b1f → #c0392b → #cb4632   (dark uses #e0553e mid + glow)
ink glyph (light)  #2b2723
ivory glyph (dark) #ece5d3

Type: Shippori Mincho 700.
