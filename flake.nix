{
  description = "Custom GeForce Now Client Named OpenNOW";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        appPackage = builtins.fromJSON (builtins.readFile ./opennow-stable/package.json);
      in
      {
        packages.default = pkgs.buildNpmPackage {
          pname = "opennow";
          version = appPackage.version;

          src = ./opennow-stable;

          npmDepsHash = "sha256-iMoYLIydDGTHgm6eMdoXb65NcPFXfWcVztmvMRHmCJs=";
          npmDepsFetcherVersion = 2;
          makeCacheWritable = true;
          forceGitDeps = true;

          npmInstallFlags = [
            "--legacy-peer-deps"
            "--no-audit"
          ];
          npmFlags = [
            "--legacy-peer-deps"
            "--no-audit"
          ];
          npmCiFlags = [
            "--legacy-peer-deps"
            "--no-audit"
          ];

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.python3
            pkgs.gcc
            pkgs.makeWrapper
            pkgs.copyDesktopItems
          ];

          buildInputs = [
            pkgs.openssl
            pkgs.zlib
            pkgs.electron
          ];

          ELECTRON_SKIP_BINARY_DOWNLOAD = "1";

          installPhase = ''
            runHook preInstall

            appdir="$TMPDIR/opennow-install"
            mkdir -p "$appdir" "$out/lib/opennow"
            if [ -f package.json ]; then
              cp package.json "$appdir/"
            fi
            if [ -f package-lock.json ]; then
              cp package-lock.json "$appdir/"
            fi
            for path in out dist dist-electron; do
              if [ -e "$path" ]; then
                cp -r "$path" "$appdir/"
              fi
            done
            if [ -d node_modules ]; then
              cp -r node_modules "$appdir/"
              if [ -f "$appdir/package.json" ]; then
                npm --prefix "$appdir" prune --omit=dev --no-audit --legacy-peer-deps
              fi
            fi
            cp -r "$appdir"/. $out/lib/opennow

            mkdir -p $out/bin
            for path in "out/main/index.js" "dist/main/index.js" "dist-electron/main.js"; do
              if [ -f "$out/lib/opennow/$path" ]; then
                MAIN_SCRIPT="$out/lib/opennow/$path"
                break
              fi
            done
            : ''${MAIN_SCRIPT:="$out/lib/opennow"}

            makeWrapper ${pkgs.electron}/bin/electron $out/bin/opennow \
              --add-flags "$MAIN_SCRIPT" \
              --set NODE_ENV production \
              --add-flags "--enable-features=WaylandWindowDecorations --platform-hint=auto"

            mkdir -p $out/share/icons/hicolor/512x512/apps
            if [ -f "src/renderer/src/assets/opennow-logo.png" ]; then
              cp src/renderer/src/assets/opennow-logo.png $out/share/icons/hicolor/512x512/apps/opennow.png
            fi
            runHook postInstall
          '';
          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "opennow";
              desktopName = "OpenNOW"; 
              genericName = "Custom GeForce Now Client Named OpenNOW";
              comment = "An open-source desktop client for GeForce NOW";
              exec = "opennow %U";
              icon = "opennow";
              keywords = [ "GFN" "GeForceNOW" "cloud" "gaming" ];
              startupWMClass = "opennow"; 
              startupNotify = true;
              categories = [ "Game" "Network" ];
            })
          ];
        };
      }
    );
}
