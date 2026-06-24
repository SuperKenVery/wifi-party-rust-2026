{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    systems.url = "github:nix-systems/default";
    bundlers = {
      url = "github:NixOS/bundlers";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    systems,
    nixpkgs,
    crane,
    ...
  } @ inputs: let
    eachSystem = f:
      nixpkgs.lib.genAttrs (import systems) (
        system:
          f (import nixpkgs {
            inherit system;
            overlays = [inputs.rust-overlay.overlays.default];
            config = {
              allowUnfree = true;
              android_sdk.accept_license = true;
            };
          })
      );

    rustToolchain = eachSystem (pkgs:
      pkgs.rust-bin.stable.latest.default.override {
        extensions = ["rust-src" "rust-analyzer"];
        targets = [
          "wasm32-unknown-unknown"
          "aarch64-linux-android"
          "aarch64-apple-ios"
          "aarch64-apple-ios-sim"
          "x86_64-apple-ios"
        ];
      });

    dioxus-cli = eachSystem (pkgs:
      pkgs.dioxus-cli.overrideAttrs (oldAttrs: {
        # postPatch = ''
        #   rm Cargo.lock
        #   cp ${./Dioxus.lock} Cargo.lock
        # '';

        # cargoDeps = pkgs.rustPlatform.importCargoLock {
        #   lockFile = ./Dioxus.lock;
        # };
      }));

    cargoLock = builtins.fromTOML (builtins.readFile ./Cargo.lock);

    wasmBindgen = eachSystem (pkgs: (pkgs.lib.findFirst
      (pkg: pkg.name == "wasm-bindgen")
      (throw "Could not find wasm-bindgen package")
      cargoLock.package));

    wasm-bindgen-cli = eachSystem (pkgs: (pkgs.buildWasmBindgenCli rec {
      src = pkgs.fetchCrate {
        pname = "wasm-bindgen-cli";
        version = wasmBindgen.${pkgs.stdenv.hostPlatform.system}.version;
        hash = "sha256-ve783oYH0TGv8Z8lIPdGjItzeLDQLOT5uv/jbFOlZpI=";
      };
      cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
        inherit src;
        inherit (src) pname version;
        hash = "sha256-EYDfuBlH3zmTxACBL+sjicRna84CvoesKSQVcYiG9P0=";
      };
    }));

    androidSdk = eachSystem (pkgs:
      (pkgs.androidenv.composeAndroidPackages {
        platformVersions = ["33" "34"];
        buildToolsVersions = ["33.0.0" "34.0.0"];
        ndkVersions = ["29.0.14206865"];
        includeEmulator = false;
        includeSources = false;
        includeSystemImages = false;
        includeNDK = true;
      }).androidsdk);

    androidCmake = eachSystem (pkgs:
      pkgs.writeShellScriptBin "android-cmake" ''
        if [ "''${1:-}" = "--build" ]; then
          exec ${pkgs.cmake}/bin/cmake "$@"
        fi

        exec ${pkgs.cmake}/bin/cmake "$@" -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-28 -DCMAKE_SYSTEM_VERSION=28
      '');
  in rec {
    bundlers = eachSystem (pkgs: let
      system = pkgs.stdenv.hostPlatform.system;
    in {
      default = inputs.bundlers.bundlers.${system}.default;
      appimage = inputs.bundlers.bundlers.${system}.toAppImage;
    });

    devShells = eachSystem (pkgs: {
      # Based on a discussion at https://github.com/oxalica/rust-overlay/issues/129
      default = pkgs.mkShell (with pkgs;
        {
          nativeBuildInputs =
            [
              sqlite
              darwin.sigtool
              binaryen
            ]
            # Use mold when we are runnning in Linux.
            ++ lib.optionals stdenv.isLinux [mold];

          buildInputs =
            [
              rustToolchain.${pkgs.stdenv.hostPlatform.system}
              cargo
              dioxus-cli.${pkgs.stdenv.hostPlatform.system}
              wasm-bindgen-cli.${pkgs.stdenv.hostPlatform.system}
              nodejs
              lld
              cmake
              pkg-config
              jdk17
              androidSdk.${pkgs.stdenv.hostPlatform.system}
            ]
            ++ (pkgs.lib.optionals pkgs.stdenv.isLinux (with pkgs; [
              alsa-lib
              dbus
              glib
              gtk3
              jack2
              libpulseaudio
              llvmPackages.libclang
              openssl
              opus
              pipewire
              webkitgtk_4_1
              xdotool
            ]));

          RUST_BACKTRACE = "1";
          RUST_LOG = "warn,wifi_party_rust=debug,vocal-model=debug";

          CMAKE_POLICY_VERSION_MINIMUM = "3.5";

          JAVA_HOME = "${jdk17}";
          ANDROID_HOME = "${androidSdk.${pkgs.stdenv.hostPlatform.system}}/libexec/android-sdk";
          NDK_HOME = "${androidSdk.${pkgs.stdenv.hostPlatform.system}}/libexec/android-sdk/ndk/29.0.14206865";
          LIBCLANG_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux "${pkgs.llvmPackages.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = pkgs.lib.optionalString pkgs.stdenv.isLinux "-DSPA_ID_INVALID=4294967295U -I${pkgs.glibc.dev}/include";
          CMAKE_TOOLCHAIN_FILE_aarch64_linux_android = "${androidSdk.${pkgs.stdenv.hostPlatform.system}}/libexec/android-sdk/ndk/29.0.14206865/build/cmake/android.toolchain.cmake";
          CMAKE_aarch64_linux_android = "${androidCmake.${pkgs.stdenv.hostPlatform.system}}/bin/android-cmake";
          APPIMAGE_EXTRACT_AND_RUN = pkgs.lib.optionalString pkgs.stdenv.isLinux "1";
        }
        // pkgs.lib.optionalAttrs (!pkgs.stdenv.isLinux) {
          OPUS_NO_PKG = "1";
        });
    });

    packages = eachSystem (pkgs: rec {
      default = app;
      app = let
        system = pkgs.stdenv.hostPlatform.system;
        craneLib = crane.mkLib pkgs;
        commonArgsBase = rec {
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = manifestFilter;
            name = "source";
          };
          # src = builtins.trace src1.outPath src1;

          nativeBuildInputs = devShells.${system}.default.nativeBuildInputs;
          buildInputs = devShells.${system}.default.buildInputs;

          LIBCLANG_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux "${pkgs.llvmPackages.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = pkgs.lib.optionalString pkgs.stdenv.isLinux "-DSPA_ID_INVALID=4294967295U -I${pkgs.glibc.dev}/include";
          cargoExtraArgs = "--locked --no-default-features --features vocal-removal,desktop,cpal-pipewire";
          doCheck = false;
        };
        cargoVendorDir = pkgs.runCommand "vendor-cargo-deps-patched" {} ''
          mkdir -p "$out"
          cp -rL --no-preserve=mode,ownership ${craneLib.vendorCargoDeps commonArgsBase}/. "$out"
          substituteInPlace "$out/config.toml" \
            --replace-fail "${craneLib.vendorCargoDeps commonArgsBase}" "$out"
          substituteInPlace "$out"/*/libspa-0.10.0/src/constants.rs \
            --replace-fail "spa_sys::SPA_ID_INVALID" "u32::MAX"
          substituteInPlace "$out"/*/pipewire-0.10.0/src/constants.rs \
            --replace-fail "pw_sys::PW_ID_ANY" "u32::MAX"
        '';
        commonArgs = commonArgsBase // {inherit cargoVendorDir;};
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        # Keep all source and /assets for building this crate
        sourceFilter = path: type:
          (craneLib.filterCargoSources path type)
          || (builtins.match ".*assets/.*" path != null);
        # Only keep Cargo.toml and Cargo.lock, for building dependencies
        manifestFilter = path: type:
          (craneLib.filterCargoSources path type)
          || (builtins.match ".*/Cargo\\..*" path != null);
        tailwind-assets = pkgs.buildNpmPackage {
          name = "tailwind-assets";
          src = ./assets;

          npmDepsHash = "sha256-1mjkLm2cjzGtxB6llUVvxBjqeLAJweYuI/6qyPCHud8=";

          # Override the build command to generate the specific file you need
          # Adjust 'input.css' to whatever your source css file is named
          buildPhase = ''
            npx @tailwindcss/cli -i tailwind.css -o tailwind_output.css
          '';

          installPhase = ''
            mkdir -p $out
            cp tailwind_output.css $out/
          '';
        };
      in
        craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;

            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = sourceFilter;
              name = "source";
            };

            postPatch = ''
              cp ${tailwind-assets}/tailwind_output.css assets/tailwind_output.css
            '';

            meta.mainProgram = "wifi-party-rust";
          }
        );
    });
  };
}
