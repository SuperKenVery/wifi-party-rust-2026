{
  pkgs,
  crane,
  devShell,
  androidSdk,
  androidCmake,
  root,
}: let
  system = pkgs.stdenv.hostPlatform.system;
  craneLib = crane.mkLib pkgs;
  sourceFilter = path: type:
    (craneLib.filterCargoSources path type)
    || (builtins.match ".*assets/.*" path != null)
    || (builtins.baseNameOf path == "Dioxus.toml")
    || (builtins.baseNameOf path == "dioxus.toml");
  manifestFilter = path: type:
    (craneLib.filterCargoSources path type)
    || (builtins.match ".*/Cargo\\..*" path != null);
  commonArgsBase = rec {
    src = pkgs.lib.cleanSourceWith {
      src = root;
      filter = manifestFilter;
      name = "source";
    };

    nativeBuildInputs = devShell.nativeBuildInputs;
    buildInputs = devShell.buildInputs;

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
  tailwind-assets = pkgs.buildNpmPackage {
    name = "tailwind-assets";
    src = root + /assets;

    npmDepsHash = "sha256-1mjkLm2cjzGtxB6llUVvxBjqeLAJweYuI/6qyPCHud8=";

    buildPhase = ''
      npx @tailwindcss/cli -i tailwind.css -o tailwind_output.css
    '';

    installPhase = ''
      mkdir -p $out
      cp tailwind_output.css $out/
    '';
  };
  bundlePackage = {
    pname,
    platformFlag,
    packageType,
    features,
  }:
    pkgs.stdenv.mkDerivation ({
      inherit pname;
      version = "0.1.0";

      src = pkgs.lib.cleanSourceWith {
        src = root;
        filter = sourceFilter;
        name = "source";
      };

      nativeBuildInputs = commonArgs.nativeBuildInputs;
      buildInputs = commonArgs.buildInputs;

      dontConfigure = true;

      inherit (commonArgs) LIBCLANG_PATH BINDGEN_EXTRA_CLANG_ARGS;
      CMAKE_POLICY_VERSION_MINIMUM = "3.5";
      JAVA_HOME = "${pkgs.jdk17}";
      ANDROID_HOME = "${androidSdk}/libexec/android-sdk";
      NDK_HOME = "${androidSdk}/libexec/android-sdk/ndk/29.0.14206865";
      CMAKE_TOOLCHAIN_FILE_aarch64_linux_android = "${androidSdk}/libexec/android-sdk/ndk/29.0.14206865/build/cmake/android.toolchain.cmake";
      CMAKE_aarch64_linux_android = "${androidCmake}/bin/android-cmake";

      postPatch = ''
        cp ${tailwind-assets}/tailwind_output.css assets/tailwind_output.css
        mkdir -p .cargo
        cp ${cargoVendorDir}/config.toml .cargo/config.toml
      '';

      buildPhase = ''
        runHook preBuild

        export HOME="$TMPDIR/home"
        export CARGO_HOME="$TMPDIR/cargo-home"
        export DX_SESSION_CACHE_DIR="$TMPDIR/dx"
        mkdir -p "$HOME" "$CARGO_HOME" "$DX_SESSION_CACHE_DIR"

        dx bundle \
          ${platformFlag} \
          --package-types ${packageType} \
          --out-dir bundle-out \
          --release \
          --no-default-features \
          --features "${features}" \
          --locked \
          --offline \
          --verbose

        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall
        mkdir -p "$out"
        cp -R bundle-out/. "$out/"
        runHook postInstall
      '';
    }
    // pkgs.lib.optionalAttrs (!pkgs.stdenv.isLinux) {
      OPUS_NO_PKG = "1";
    });
in
  rec {
    default = app;
    app =
      craneLib.buildPackage (
        commonArgs
        // {
          inherit cargoArtifacts;

          src = pkgs.lib.cleanSourceWith {
            src = root;
            filter = sourceFilter;
            name = "source";
          };

          postPatch = ''
            cp ${tailwind-assets}/tailwind_output.css assets/tailwind_output.css
          '';

          meta.mainProgram = "wifi-party-rust";
        }
      );
  }
  // pkgs.lib.optionalAttrs pkgs.stdenv.isDarwin {
    macos-dmg = bundlePackage {
      pname = "wifi-party-rust-dmg";
      platformFlag = "--macos";
      packageType = "dmg";
      features = "vocal-removal desktop";
    };
  }
  // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
    android-apk = bundlePackage {
      pname = "wifi-party-rust-apk";
      platformFlag = "--android";
      packageType = "apk";
      features = "mobile";
    };
  }
