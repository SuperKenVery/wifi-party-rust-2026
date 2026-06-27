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
  macosHdiutil = pkgs.writeShellScriptBin "hdiutil" ''
    exec /usr/bin/hdiutil "$@"
  '';
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
  dioxusCliSrc = pkgs.fetchCrate {
    pname = "dioxus-cli";
    version = "0.7.9";
    hash = "sha256-tLMtUlohSJt3okdJh+ARweQNGmzj/vYiNl8iZhDbSAc=";
  };
  androidGradleProject = pkgs.stdenvNoCC.mkDerivation {
    pname = "wifi-party-rust-android-gradle-project";
    version = "0.1.0";
    src = dioxusCliSrc;

    nativeBuildInputs = [pkgs.gradle_9 androidSdk];
    gradleBuildTask = "help";
    ANDROID_HOME = "${androidSdk}/libexec/android-sdk";

    postPatch = ''
      shopt -s dotglob
      mkdir "$TMPDIR/android-gradle"
      cp -R assets/android/gen/. "$TMPDIR/android-gradle/"
      rm -rf ./*
      cp -R "$TMPDIR/android-gradle"/. .
      printf 'sdk.dir=%s\n' '${androidSdk}/libexec/android-sdk' > local.properties

      mv app/build.gradle.kts.hbs app/build.gradle.kts
      substituteInPlace app/build.gradle.kts \
        --replace-fail "{{ application_id }}" "com.ken.WifiPartyRust" \
        --replace-fail "{{ compile_sdk }}" "34" \
        --replace-fail "{{ min_sdk }}" "24" \
        --replace-fail "{{ target_sdk }}" "34"
      sed -i '/{{#each gradle_plugins}}/,/{{\/each}}/d' app/build.gradle.kts
      sed -i '/{{#each gradle_dependencies}}/,/{{\/each}}/d' app/build.gradle.kts
      sed -i '/{{#if android_bundle}}/,/{{\/if}}/d' app/build.gradle.kts
      sed -i '/{{#if android_bundle}}/d; /{{\/if}}/d' app/build.gradle.kts
      cat >> app/build.gradle.kts <<'EOF'

      androidComponents {
          beforeVariants(selector().all()) {
              it.enableAndroidTest = false
              it.enableUnitTest = false
          }
      }

      val nixAapt2 by configurations.creating

      dependencies {
          nixAapt2("com.android.tools.build:aapt2:8.7.0-12006047:linux")
      }
      EOF

      rm app/src/main/AndroidManifest.xml.hbs
      cat > app/src/main/AndroidManifest.xml <<'EOF'
      <?xml version="1.0" encoding="utf-8"?>
      <manifest xmlns:android="http://schemas.android.com/apk/res/android">
          <application android:hasCode="true" android:label="@string/app_name">
              <activity android:name="dev.dioxus.main.MainActivity" android:exported="true">
                  <intent-filter>
                      <action android:name="android.intent.action.MAIN" />
                      <category android:name="android.intent.category.LAUNCHER" />
                  </intent-filter>
              </activity>
          </application>
      </manifest>
      EOF

      mv app/src/main/res/values/strings.xml.hbs app/src/main/res/values/strings.xml
      substituteInPlace app/src/main/res/values/strings.xml \
        --replace-quiet "{{ app_name }}" "WifiPartyRust"
    '';
  };
  bundlePackage = {
    pname,
    platformFlag,
    packageType,
    features,
    gradleDeps ? null,
  }:
    pkgs.stdenv.mkDerivation ({
      inherit pname;
      version = "0.1.0";

      src = pkgs.lib.cleanSourceWith {
        src = root;
        filter = sourceFilter;
        name = "source";
      };

      nativeBuildInputs =
        commonArgs.nativeBuildInputs
        ++ pkgs.lib.optionals (gradleDeps != null) [pkgs.gradle_9]
        ++ pkgs.lib.optionals (pkgs.stdenv.isDarwin && packageType == "dmg") [macosHdiutil];
      buildInputs = commonArgs.buildInputs;

      mitmCache = pkgs.lib.optionalString (gradleDeps != null) "${gradleDeps}";

      dontConfigure = gradleDeps == null;
      dontUseCmakeConfigure = gradleDeps != null;

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

        ${pkgs.lib.optionalString (gradleDeps != null) ''
          dioxus_gradle="$TMPDIR/dioxus-gradle"
          {
            echo "#!${pkgs.runtimeShell}"
            printf "exec %q" "${pkgs.gradle_9}/bin/gradle"
            for arg in "''${gradleFlagsArray[@]}"; do
              printf " %q" "$arg"
            done
            printf " %q" "-Pandroid.aapt2FromMavenOverride=${androidSdk}/libexec/android-sdk/build-tools/34.0.0/aapt2"
            echo ' "$@"'
          } > "$dioxus_gradle"
          chmod +x "$dioxus_gradle"
          export DIOXUS_GRADLE="$dioxus_gradle"
        ''}

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
      features = "vocal-removal,desktop";
    };
  }
  // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (rec {
    android-apk = bundlePackage {
      pname = "wifi-party-rust-apk";
      platformFlag = "--android";
      packageType = "apk";
      features = "mobile";
      gradleDeps = android-gradle-deps;
    };
    android-gradle-deps = pkgs.gradle_9.fetchDeps {
      pname = "wifi-party-rust-android-gradle";
      data = ./android-gradle-deps.json;
      pkg = androidGradleProject;
    };
  })
