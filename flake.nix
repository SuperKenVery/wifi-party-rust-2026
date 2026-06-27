{
  nixConfig = {
    extra-substituters = [
      "https://nix-binary-cache.ken.com.im/ken"
      "https://nix-binary-cache.ken.com.im/wifi-party"
    ];
    extra-trusted-public-keys = [
      "ken.com.im:br/oG6ywHr+tGvmUpZEA5mVYSNZgrNrFflazAEI+AK4="
      "wifi-party:H2KMuBabLl9WHuBWw0cgXmYDdt82K7a1gmuvfKU9shY="
    ];
  };

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
          "aarch64-linux-android" "x86_64-linux-android"
          "aarch64-apple-ios" "aarch64-apple-ios-sim" "x86_64-apple-ios"
        ];
      });

    dioxus-cli = eachSystem (pkgs:
      pkgs.dioxus-cli.overrideAttrs (oldAttrs: {
        postPatch =
          (oldAttrs.postPatch or "")
          + ''
            substituteInPlace src/build/android.rs \
              --replace-fail 'Ok(self.root_dir().join(gradle_exec_name))' \
                'Ok(std::env::var_os("DIOXUS_GRADLE").map(PathBuf::from).unwrap_or_else(|| self.root_dir().join(gradle_exec_name)))'
          '';
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
              binaryen
            ]
            # Use mold when we are runnning in Linux.
            ++ lib.optionals stdenv.isLinux [mold]
            ++ lib.optionals stdenv.isDarwin [darwin.sigtool];

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

    packages = eachSystem (pkgs: let
      system = pkgs.stdenv.hostPlatform.system;
    in
      import ./nix/packages.nix {
        inherit pkgs crane;
        root = ./.;
        devShell = devShells.${system}.default;
        androidSdk = androidSdk.${system};
        androidCmake = androidCmake.${system};
      });
  };
}
