#!/usr/bin/env bash
# snap/snapcraft.yaml and the snap's desktop entry.
set -eu
cd "$(dirname "$0")/../.."
fail() { echo "FAIL: $*" >&2; exit 1; }

ruby -ryaml -e '
  s = YAML.load_file("snap/snapcraft.yaml")
  { "name" => "cecilia", "base" => "core24", "confinement" => "strict", "grade" => "stable",
    "adopt-info" => "cecilia" }.each { |k, v| abort "#{k} is not #{v}" unless s[k] == v }
  gui = s["apps"]["gui"] or abort "no gui app"
  abort "gui command" unless gui["command"] == "bin/cecilia --gui"
  %w[desktop wayland x11 opengl audio-playback audio-record home].each do |p|
    abort "gui lacks plug #{p}" unless gui["plugs"].include?(p)
  end
  abort "no cecilia app" unless s["apps"]["cecilia"]["command"] == "bin/cecilia"
' || fail "snapcraft.yaml"

cmp snap/gui/cecilia.svg packaging/linux/cecilia.svg || fail "snap icon differs from packaging/linux/cecilia.svg"
grep -q '^Exec=cecilia\.gui' snap/gui/cecilia.desktop || fail "desktop Exec is not cecilia.gui"
if command -v desktop-file-validate >/dev/null; then
    # ${SNAP}/... is how snaps name their icon; the validator wants a plain name, so swap it.
    tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
    sed 's|^Icon=.*|Icon=cecilia|' snap/gui/cecilia.desktop > "$tmp/cecilia.desktop"
    desktop-file-validate "$tmp/cecilia.desktop" || fail "desktop-file-validate"
    # shellcheck disable=SC2016
    grep -q '^Icon=${SNAP}/meta/gui/cecilia.svg$' snap/gui/cecilia.desktop || fail "snap icon path"
fi
echo "snap: ok"
