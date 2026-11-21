// Fills download links on the website with the files of the latest GitHub release, and marks
// the visitor's operating system on <html data-os="linux|macos|windows|unknown">.
//
// Markup it understands (every link already points at the releases page, so the page works
// without this script or when the GitHub API can't be reached):
//   data-release="version"      text set to the version, e.g. 0.50.0
//   data-release="date"         text set to the release date
//   data-release="notes"        href set to the release page
//   data-asset="KEY"            href set to that file; its [data-asset-row] is hidden when the
//                               release has no such file
//   data-asset-size="KEY"       text set to the file's size
//   data-download="primary"     href and text set to the best file for the visitor's system
//   data-os-name                text set to the visitor's system ("Linux", "macOS", "Windows")
(() => {
  "use strict";

  const API = "https://api.github.com/repos/kjmcnamara1/cecilia/releases/latest";
  const CACHE_KEY = "cecilia-latest-release";
  const CACHE_MS = 60 * 60 * 1000;

  const ASSETS = {
    dmg: /-universal-apple-darwin\.dmg$/,
    "mac-arm-tar": /-aarch64-apple-darwin\.tar\.gz$/,
    "mac-x64-tar": /-x86_64-apple-darwin\.tar\.gz$/,
    appimage: /-x86_64\.AppImage$/,
    deb: /_amd64\.deb$/,
    rpm: /\.x86_64\.rpm$/,
    flatpak: /\.flatpak$/,
    snap: /_amd64\.snap$/,
    "linux-tar": /-x86_64-unknown-linux-gnu\.tar\.gz$/,
    "win-setup": /-Setup\.exe$/,
    "win-zip": /-x86_64-pc-windows-msvc\.zip$/,
    "install-sh": /^install\.sh$/,
    sums: /^SHA256SUMS$/,
  };

  const SYSTEMS = {
    linux: { name: "Linux", primary: "appimage", label: "Download the AppImage" },
    macos: { name: "macOS", primary: "dmg", label: "Download for macOS" },
    windows: { name: "Windows", primary: "win-setup", label: "Download for Windows" },
  };

  function detectOS() {
    const ua = navigator.userAgent;
    const platform = (navigator.userAgentData && navigator.userAgentData.platform) || "";
    // Phones, tablets and Chromebooks can't run the desktop builds.
    if (/Android|iPhone|iPad|iPod|CrOS/i.test(ua) || /Android|Chrome OS/i.test(platform)) {
      return "unknown";
    }
    if (/Mac/i.test(platform || ua)) {
      // iPadOS reports itself as a Mac, but has a touch screen.
      return navigator.maxTouchPoints > 1 ? "unknown" : "macos";
    }
    if (/Win/i.test(platform || ua)) return "windows";
    if (/Linux|X11/i.test(platform || ua)) return "linux";
    return "unknown";
  }

  function formatSize(bytes) {
    return bytes >= 1e6 ? `${Math.round(bytes / 1e6)} MB` : `${Math.max(1, Math.round(bytes / 1e3))} kB`;
  }

  async function latestRelease() {
    try {
      const cached = JSON.parse(sessionStorage.getItem(CACHE_KEY) || "null");
      if (cached && Date.now() - cached.at < CACHE_MS) return cached.release;
    } catch (_) {
      // Storage may be disabled; fetch instead.
    }
    const response = await fetch(API, { headers: { Accept: "application/vnd.github+json" } });
    if (!response.ok) throw new Error(`GitHub API: ${response.status}`);
    const data = await response.json();
    const release = {
      version: data.tag_name.replace(/^v/, ""),
      date: data.published_at,
      url: data.html_url,
      assets: data.assets.map((a) => ({ name: a.name, url: a.browser_download_url, size: a.size })),
    };
    try {
      sessionStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), release }));
    } catch (_) {
      // Not cached; harmless.
    }
    return release;
  }

  function findAssets(release) {
    const found = {};
    for (const [key, pattern] of Object.entries(ASSETS)) {
      const asset = release.assets.find((a) => pattern.test(a.name));
      if (asset) found[key] = asset;
    }
    return found;
  }

  function applyRelease(release, os) {
    const assets = findAssets(release);
    const date = new Date(release.date).toLocaleDateString(undefined, {
      year: "numeric",
      month: "long",
      day: "numeric",
    });
    document.querySelectorAll('[data-release="version"]').forEach((el) => (el.textContent = release.version));
    document.querySelectorAll('[data-release="date"]').forEach((el) => (el.textContent = date));
    document.querySelectorAll('[data-release="notes"]').forEach((el) => (el.href = release.url));
    document.querySelectorAll("[data-asset]").forEach((el) => {
      const asset = assets[el.dataset.asset];
      if (asset) {
        el.href = asset.url;
      } else {
        const row = el.closest("[data-asset-row]");
        if (row) row.hidden = true;
      }
    });
    document.querySelectorAll("[data-asset-size]").forEach((el) => {
      const asset = assets[el.dataset.assetSize];
      if (asset) el.textContent = formatSize(asset.size);
    });
    const system = SYSTEMS[os];
    const primary = system && assets[system.primary];
    if (primary) {
      document.querySelectorAll('[data-download="primary"]').forEach((el) => {
        el.href = primary.url;
        el.textContent = system.label;
      });
    }
  }

  function init() {
    const os = detectOS();
    document.documentElement.dataset.os = os;
    if (SYSTEMS[os]) {
      document.querySelectorAll("[data-os-name]").forEach((el) => (el.textContent = SYSTEMS[os].name));
    }
    if (!document.querySelector("[data-release], [data-asset], [data-download]")) return;
    latestRelease()
      .then((release) => applyRelease(release, os))
      .catch(() => {
        // Leave the links pointing at the releases page.
      });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
