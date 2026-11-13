#!/usr/bin/env bash
# The Flatpak manifest and AppStream metainfo.
set -eu
cd "$(dirname "$0")/../.."
fail() { echo "FAIL: $*" >&2; exit 1; }
ID=io.github.kjmcnamara1.cecilia
manifest=packaging/flatpak/$ID.yml
meta=packaging/linux/$ID.metainfo.xml

ruby -ryaml -e '
  m = YAML.load_file(ARGV[0])
  abort "app-id" unless m["app-id"] == ARGV[1]
  abort "command" unless m["command"] == "cecilia"
  args = m["finish-args"]
  %w[--socket=wayland --socket=fallback-x11 --device=dri --socket=pulseaudio --filesystem=home].each do |a|
    abort "finish-args lacks #{a}" unless args.include?(a)
  end
' "$manifest" $ID || fail "manifest"

xmllint --noout "$meta" || fail "metainfo is not well-formed"
if command -v appstreamcli >/dev/null; then
    appstreamcli validate --no-net "$meta" || fail "appstreamcli validate"
else
    echo "appstreamcli not installed: validation skipped"
fi

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
grep -q "<release version=\"$version\"" "$meta" || fail "metainfo has no release $version (just bump keeps it)"
echo "flatpak: ok"
