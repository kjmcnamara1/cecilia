{
  description = "cecilia: keyboard-first editor and player for guitar tab, notation and Gregorian chant";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # wgpu and winit open these with dlopen at run time, so they must be on the library
      # path rather than merely linked.
      runtimeLibsFor =
        pkgs:
        pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (
          with pkgs;
          [
            vulkan-loader
            libGL
            wayland
            libxkbcommon
            libx11
            libxcursor
            libxi
            libxrandr
          ]
        );
      # The version lives in Cargo.toml only (`mise run bump` sets it), so read it from there.
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
    in
    {
      packages = forAll (
        pkgs:
        let
          inherit (pkgs) lib stdenv;
          runtimeLibs = runtimeLibsFor pkgs;
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "cecilia";
            inherit version;
            src = lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "cecilia" ];
            # The workspace tests need the test corpus and a GPU; CI runs them.
            doCheck = false;

            nativeBuildInputs = [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ] ++ lib.optionals stdenv.hostPlatform.isLinux [ pkgs.makeWrapper ];
            buildInputs = lib.optionals stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];

            # Same layout as install.sh and the distribution packages: desktop entry, icon,
            # file types, AppStream metadata, licences, docs.
            # The wrapper also covers non-NixOS systems: their GPU drivers are built against the
            # distribution's libraries and cannot be loaded here, so when NixOS's driver
            # directory is missing it falls back to nixpkgs' Mesa (AMD, Intel, software).
            # NVIDIA's proprietary driver still needs nixGL there.
            postInstall = lib.optionalString stdenv.hostPlatform.isLinux ''
              mv $out/bin/cecilia $TMPDIR/cecilia
              sh packaging/linux/stage.sh "" $out $TMPDIR/cecilia
              for b in $out/bin/*; do
                wrapProgram $b --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs} \
                  --run 'if [ ! -e /run/opengl-driver ]; then
                    export LD_LIBRARY_PATH=${pkgs.mesa}/lib:$LD_LIBRARY_PATH
                    export __EGL_VENDOR_LIBRARY_DIRS=''${__EGL_VENDOR_LIBRARY_DIRS:-${pkgs.mesa}/share/glvnd/egl_vendor.d}
                    export VK_DRIVER_FILES=''${VK_DRIVER_FILES:-''${VK_ICD_FILENAMES:-${pkgs.mesa}/share/vulkan/icd.d}}
                  fi'
              done
            '';

            meta = {
              description = "Tablature, notation and chant editor and player";
              homepage = "https://github.com/kjmcnamara1/cecilia";
              license = with lib.licenses; [
                mit
                asl20
              ];
              mainProgram = "cecilia";
            };
          };
        }
      );

      apps = forAll (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/cecilia";
          meta.description = "Tablature, notation and chant editor and player";
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            mise
          ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibsFor pkgs);
        };
      });
    };
}
