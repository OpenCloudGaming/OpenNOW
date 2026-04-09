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
        electron = pkgs.electron;

        nativeDeps = [
          pkgs.pkg-config
          pkgs.python3
          pkgs.gcc
          pkgs.makeWrapper
          pkgs.copyDesktopItems
        ];

        runtimeDeps = [
          pkgs.openssl
          pkgs.zlib
        ]
        ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (
          with pkgs.darwin.apple_sdk.frameworks;
          [
            CoreGraphics
            CoreVideo
            AppKit
            IOKit
          ]
        );

      in
      {
        packages.default = pkgs.buildNpmPackage {
          pname = "opennow";
          version = "1.0.0";

          src = ./opennow-stable;

          npmDepsHash = "sha256-rd6DcBubFSl2MFWdkVlj+2WGI7vxhr361sMzyjcrst0=";

          ELECTRON_SKIP_BINARY_DOWNLOAD = "1";

          nativeBuildInputs = nativeDeps;
          buildInputs = runtimeDeps;

          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "opennow";
              exec = "opennow %U";
              icon = "opennow";
              desktopName = "OpenNOW";
              genericName = "Cloud Gaming Client";
              categories = [
                "Game"
                "Network"
              ];
              terminal = false;
            })
          ];

          npmBuild = "npm run build";

          installPhase = ''
            runHook preInstall

            mkdir -p $out/lib/opennow
            cp -r . $out/lib/opennow

            mkdir -p $out/bin

            for path in "out/main/index.js" "dist/main/index.js" "dist-electron/main.js"; do
              if [ -f "$out/lib/opennow/$path" ]; then
                MAIN_SCRIPT="$out/lib/opennow/$path"
                break
              fi
            done

            : ''${MAIN_SCRIPT:="$out/lib/opennow"}

            makeWrapper ${electron}/bin/electron $out/bin/opennow \
              --add-flags "$MAIN_SCRIPT" \
              --set NODE_ENV production \
              --inherit-argv0 \
              --add-flags "--enable-features=WaylandWindowDecorations --platform-hint=auto"

            mkdir -p $out/share/icons/hicolor/512x512/apps
            if [ -f "src/renderer/src/assets/opennow-logo.png" ]; then
              cp src/renderer/src/assets/opennow-logo.png $out/share/icons/hicolor/512x512/apps/opennow.png
            fi

            runHook postInstall
          '';

          meta = with pkgs.lib; {
            description = "OpenCloudGaming OpenNOW Client";
            homepage = "https://github.com/OpenCloudGaming/OpenNOW";
            license = licenses.mit;
            platforms = platforms.linux ++ platforms.darwin;
            mainProgram = "opennow";
          };
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          shellHook = ''
            export ELECTRON_SKIP_BINARY_DOWNLOAD=1
          '';
        };
      }
    );
}
