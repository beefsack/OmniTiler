{
  description = "OmniTiler packages";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/54ba4bcec4043e72a4006d825e0d7aff5562008f";

  outputs = { self, nixpkgs }:
    let
      systems = [
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;

      kwinScriptSource = pkgs: pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [
          # License notices travel with the sources.
          ./LICENSE
          ./LICENSES
          ./kwin/metadata.json
          ./kwin/package.json
          ./kwin/package-lock.json
          ./kwin/src
          ./kwin/contents/config/main.xml
          ./kwin/contents/ui/config.ui
        ];
      };

      traySource = pkgs: pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [
          ./LICENSE
          ./LICENSES
          # Generated union third-party notice (both shipping Rust roots).
          ./THIRD-PARTY-NOTICES
          ./Cargo.toml
          ./Cargo.lock
          ./crates
          ./test-fixtures
          ./assets/icons/omnitiler.svg
          ./home-manager-module.nix
        ];
      };

      plannerDbusServiceSource = pkgs: pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = ./nix/com.omnitiler.Planner.service;
      };

      nativeEffectSource = pkgs: pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [
          # License notices travel with every source fileset: the effect
          # binary links the MIT staticlib into GPL-covered plugin sources.
          ./LICENSE
          ./LICENSES
          # Generated union third-party notice (both shipping Rust roots).
          # Shared with the settings-only fileset below, which compiles no
          # Rust and installs no notice.
          ./THIRD-PARTY-NOTICES
          # Complete native-effect subtree for BUILD_TESTING (effect, KCM,
          # all test sources, validators, headers) plus the script metadata
          # referenced by discovery validation. Individual file lists drift;
          # the subtree keeps the fileset exact.
          ./kwin/native-effect
          ./kwin/metadata.json
          # All workspace manifests/targets: Cargo requires every member
          # manifest+targets to resolve the workspace even when building
          # only -p omnitiler. Unrelated trees (kwin script
          # sources, tray assets, test-fixtures) stay excluded.
          ./Cargo.toml
          ./Cargo.lock
          ./crates/tiler-core
          ./crates/tiler-protocol
          ./crates/omnitiler
          ./crates/tiler-windows
          ./crates/tiler-macos
          ./crates/tiler-kde-settings-exporter
          ./crates/tiler-live-fixture
        ];
      };

      mkNativeEffect =
        { pkgs
        , kwin ? pkgs.kdePackages.kwin
        , withTests ? false
        }:
        let
          kde = pkgs.kdePackages;
          kwinDev = kwin.dev;
        in
        pkgs.stdenv.mkDerivation {
          pname = "omnitiler-native-effect";
          version = "0.1.0";
          meta.license = pkgs.lib.licenses.gpl2Plus;
          src = nativeEffectSource pkgs;
          sourceRoot = "source/kwin/native-effect";

          nativeBuildInputs = [
            pkgs.cmake
            pkgs.ninja
            pkgs.pkg-config
            kde.extra-cmake-modules
          ];
          buildInputs = [
            kwin
            kwinDev
            pkgs.qt6Packages.qtbase
            kde.kcolorscheme
            kde.kconfig
            kde.kcoreaddons
            kde.kcmutils
            kde.ki18n
            kde.kwidgetsaddons
          ];
          dontWrapQtApps = true;

          cmakeFlags = [
            "-DBUILD_TESTING=${if withTests then "ON" else "OFF"}"
            "-DOMNITILER_BUILD_EFFECT=ON"
            # withTests builds the original full tree (settings ON); the
            # shipping derivation stays effect-only (settings OFF).
            "-DOMNITILER_BUILD_SETTINGS=${if withTests then "ON" else "OFF"}"
            "-DKDE_INSTALL_PLUGINDIR=lib/qt-6/plugins"
            "-DKWin_DIR=${kwinDev}/lib/cmake/KWin"
          ];

          # License notices in the installed output. $src is the
          # fileset store path (postInstall runs from the CMake build
          # dir, not sourceRoot, so no relative path is used).
          postInstall = ''
            mkdir -p "$out/share/licenses/omnitiler-native-effect"
            cp "$src/LICENSE" "$out/share/licenses/omnitiler-native-effect/LICENSE"
            cp "$src/LICENSES/GPL-2.0-or-later.txt" "$out/share/licenses/omnitiler-native-effect/GPL-2.0-or-later.txt"
            cp "$src/THIRD-PARTY-NOTICES" "$out/share/licenses/omnitiler-native-effect/THIRD-PARTY-NOTICES"
          '';

          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            test -f "$out/lib/qt-6/plugins/kwin/effects/plugins/omnitiler-active-border.so"
            grep -Fq "Permission is hereby granted" "$out/share/licenses/omnitiler-native-effect/LICENSE"
            grep -Fq "GNU GENERAL PUBLIC LICENSE" "$out/share/licenses/omnitiler-native-effect/GPL-2.0-or-later.txt"
            grep -Fq "THIRD-PARTY NOTICES FOR OMNITILER" "$out/share/licenses/omnitiler-native-effect/THIRD-PARTY-NOTICES"
            ${if withTests then ''
              # Test-only derivation: the full tree ships both KCMs as test
              # artifacts alongside the effect.
              test -f "$out/lib/qt-6/plugins/kwin/effects/configs/omnitiler-active-border_config.so"
              test -f "$out/lib/qt-6/plugins/kwin/scripts/configs/omnitiler-kwin_config.so"
            '' else ''
              # Shipping derivation: effect only, never the KCMs.
              test ! -e "$out/lib/qt-6/plugins/kwin/effects/configs/omnitiler-active-border_config.so"
              test ! -e "$out/lib/qt-6/plugins/kwin/scripts/configs/omnitiler-kwin_config.so"
            ''}
            test ! -e "$out/lib/qt-6/plugins/kwin/effects/plugins/omnitiler-drag-oracle.so"
            test ! -e "$out/lib/qt-6/plugins/kwin/scripts/configs/omnitiler-drag-oracle_config.so"
            ${if withTests then ''
              # Hermetic full gates: offscreen platform, isolated config
              # home; the test binaries isolate the session bus themselves.
              # installCheck runs with the build directory as cwd, so invoke
              # ctest directly. This is the 34-test suite (effect logic,
              # presentation consumer, facts/gesture transports, KCM
              # shortcut/config, metadata validation, unified lifecycle).
              export QT_QPA_PLATFORM=offscreen
              export XDG_CONFIG_HOME="$NIX_BUILD_TOP/check-home"
              mkdir -p "$XDG_CONFIG_HOME"
              ctest --output-on-failure
            '' else ""}
            runHook postInstallCheck
          '';
        };

      mkNativeSettings =
        { pkgs
        , withTests ? false
        }:
        let
          kde = pkgs.kdePackages;
        in
        pkgs.stdenv.mkDerivation {
          pname = "omnitiler-native-settings";
          version = "0.1.0";
          meta.license = pkgs.lib.licenses.mit;
          src = nativeEffectSource pkgs;
          sourceRoot = "source/kwin/native-effect";

          nativeBuildInputs = [
            pkgs.cmake
            pkgs.ninja
            pkgs.pkg-config
            kde.extra-cmake-modules
          ];
          buildInputs = [
            pkgs.qt6Packages.qtbase
            kde.kconfig
            kde.kcoreaddons
            kde.kcmutils
            kde.ki18n
            kde.kwidgetsaddons
          ];
          dontWrapQtApps = true;

          cmakeFlags = [
            "-DBUILD_TESTING=${if withTests then "ON" else "OFF"}"
            "-DOMNITILER_BUILD_EFFECT=OFF"
            "-DOMNITILER_BUILD_SETTINGS=ON"
            "-DKDE_INSTALL_PLUGINDIR=lib/qt-6/plugins"
          ];

          # MIT notice in the installed output ($src: fileset store path;
          # postInstall runs from the CMake build dir, not sourceRoot).
          postInstall = ''
            mkdir -p "$out/share/licenses/omnitiler-native-settings"
            cp "$src/LICENSE" "$out/share/licenses/omnitiler-native-settings/LICENSE"
          '';

          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            grep -Fq "Permission is hereby granted" "$out/share/licenses/omnitiler-native-settings/LICENSE"
            test -f "$out/lib/qt-6/plugins/kwin/effects/configs/omnitiler-active-border_config.so"
            test -f "$out/lib/qt-6/plugins/kwin/scripts/configs/omnitiler-kwin_config.so"
            test ! -e "$out/lib/qt-6/plugins/kwin/effects/plugins/omnitiler-active-border.so"
            test ! -e "$out/lib/qt-6/plugins/kwin/effects/plugins/omnitiler-drag-oracle.so"
            test ! -e "$out/lib/qt-6/plugins/kwin/scripts/configs/omnitiler-drag-oracle_config.so"
            ${if withTests then ''
              # Hermetic settings gates: offscreen platform, isolated config
              # home; the test binaries isolate the session bus themselves.
              # Effect-gated tests (logic, group, metadata, Rust FFI) are
              # configured out of this settings-only build.
              export QT_QPA_PLATFORM=offscreen
              export XDG_CONFIG_HOME="$NIX_BUILD_TOP/check-home"
              mkdir -p "$XDG_CONFIG_HOME"
              ctest --output-on-failure
            '' else ""}
            runHook postInstallCheck
          '';
        };

      mkKwinScript =
        { pkgs
        , sourceRev ? "local-dev"
        }:
        pkgs.buildNpmPackage {
          pname = "omnitiler-kwin";
          version = "0.1.0";
          meta.license = pkgs.lib.licenses.mit;
          src = kwinScriptSource pkgs;
          sourceRoot = "source/kwin";
          npmDepsHash = "sha256-6V4SnPLw4sqYE5QRtx6vr8UQ7nHaLq1nAOgzR1P6bGY=";
          # Installed build only; ordinary npm build stays define-free.
          npmBuildScript = "build:installed";
          # Shared compile-time identity: self.rev or local-dev.
          env.OMNITILER_SOURCE_REV = sourceRev;

          installPhase = ''
            runHook preInstall
            installRoot="$out/share/kwin/scripts/omnitiler-kwin"
            mkdir -p "$installRoot"
            cp -a metadata.json contents "$installRoot/"
            # MIT notice in the installed output ($src: fileset store path).
            mkdir -p "$out/share/licenses/omnitiler-kwin"
            cp "$src/LICENSE" "$out/share/licenses/omnitiler-kwin/LICENSE"
            # Static build-id marker, no paths.
            printf '%s\n' "package=omnitiler-kwin" "version=0.1.0" "source=${sourceRev}" > "$installRoot/build-id"
            runHook postInstall
          '';

          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            grep -Fq "Permission is hereby granted" "$out/share/licenses/omnitiler-kwin/LICENSE"
            installRoot="$out/share/kwin/scripts/omnitiler-kwin"
            grep -Fx "package=omnitiler-kwin" "$installRoot/build-id"
            grep -Fx "version=0.1.0" "$installRoot/build-id"
            grep -Fx "source=${sourceRev}" "$installRoot/build-id"
            # Script-only package keeps the qualified native script KCM
            # reference; the Configure page resolves only alongside the
            # companion ABI-matched native-effect delivery.
            grep -F "kwin/scripts/configs/omnitiler-kwin_config" "$installRoot/metadata.json"
            runHook postInstallCheck
          '';
        };

      mkTray =
        { pkgs
        , sourceRev ? "local-dev"
        }:
        pkgs.rustPlatform.buildRustPackage {
          pname = "omnitiler";
          version = "0.1.0";
          meta.license = pkgs.lib.licenses.mit;
          src = traySource pkgs;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "omnitiler" ];
          buildInputs = [ pkgs.kdePackages.kcmutils ];
          dontWrapQtApps = true;
          env.OMNITILER_KCMSHELL6 =
            "${pkgs.kdePackages.kcmutils}/bin/kcmshell6";
          env.OMNITILER_KWRITECONFIG6 =
            "${pkgs.kdePackages.kconfig}/bin/kwriteconfig6";
          env.OMNITILER_KREADCONFIG6 =
            "${pkgs.kdePackages.kconfig}/bin/kreadconfig6";
          # Shared compile-time identity: self.rev or local-dev.
          env.OMNITILER_SOURCE_REV = sourceRev;
          preCheck = ''
            export HOME="$NIX_BUILD_TOP"
          '';
          postInstall = ''
            mkdir -p "$out/share/icons/hicolor/scalable/apps"
            cp ${./assets/icons/omnitiler.svg} "$out/share/icons/hicolor/scalable/apps/omnitiler.svg"
            mkdir -p "$out/share/dbus-1/services"
            substitute "${plannerDbusServiceSource pkgs}/nix/com.omnitiler.Planner.service" "$out/share/dbus-1/services/com.omnitiler.Planner.service" --replace-fail "@out@" "$out"
            # Static build-id marker, no paths.
            mkdir -p "$out/share/omnitiler"
            printf '%s\n' "package=omnitiler" "version=0.1.0" "source=${sourceRev}" > "$out/share/omnitiler/build-id"
            # MIT notice in the installed output ($src: fileset store path).
            mkdir -p "$out/share/licenses/omnitiler"
            cp "$src/LICENSE" "$out/share/licenses/omnitiler/LICENSE"
            cp "$src/THIRD-PARTY-NOTICES" "$out/share/licenses/omnitiler/THIRD-PARTY-NOTICES"
          '';
          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            grep -Fq "Permission is hereby granted" "$out/share/licenses/omnitiler/LICENSE"
            grep -Fq "THIRD-PARTY NOTICES FOR OMNITILER" "$out/share/licenses/omnitiler/THIRD-PARTY-NOTICES"
            test -s "$out/share/icons/hicolor/scalable/apps/omnitiler.svg"
            test -s "$out/share/dbus-1/services/com.omnitiler.Planner.service"
            grep -Fx "[D-BUS Service]" "$out/share/dbus-1/services/com.omnitiler.Planner.service"
            grep -Fx "Name=com.omnitiler.Planner" "$out/share/dbus-1/services/com.omnitiler.Planner.service"
            grep -Fx "Exec=$out/bin/omnitiler planner-service" "$out/share/dbus-1/services/com.omnitiler.Planner.service"
            grep -Fx "SystemdService=omnitiler-planner.service" "$out/share/dbus-1/services/com.omnitiler.Planner.service"
            grep -Fx "package=omnitiler" "$out/share/omnitiler/build-id"
            grep -Fx "version=0.1.0" "$out/share/omnitiler/build-id"
            grep -Fx "source=${sourceRev}" "$out/share/omnitiler/build-id"
            runHook postInstallCheck
          '';
        };
    in
    {
      lib = {
        inherit mkKwinScript mkNativeEffect mkNativeSettings mkTray;
      };

      nixosModules.default = { config, lib, pkgs, ... }:
        import ./nixos-module.nix {
          inherit config lib pkgs;
          kwinScript = self.lib.mkKwinScript {
            inherit pkgs;
            sourceRev = self.rev or "local-dev";
          };
          nativeEffect = self.lib.mkNativeEffect { inherit pkgs; };
          nativeSettings = self.lib.mkNativeSettings { inherit pkgs; };
        };

      homeManagerModules.default = { config, lib, pkgs, ... }:
        import ./home-manager-module.nix {
          inherit config lib pkgs;
          trayPackage = self.lib.mkTray {
            inherit pkgs;
            sourceRev = self.rev or "local-dev";
          };
        };

      checks = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          kwinScript = self.lib.mkKwinScript {
            inherit pkgs;
            sourceRev = self.rev or "local-dev";
          };
          nativeEffect = self.lib.mkNativeEffect { inherit pkgs; };
          nativeSettings = self.lib.mkNativeSettings { inherit pkgs; };
          tray = self.lib.mkTray {
            inherit pkgs;
            sourceRev = self.rev or "local-dev";
          };
          enabledNixos = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              self.nixosModules.default
              { programs.omnitiler.enable = true; }
            ];
          };
          disabledNixos = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [ self.nixosModules.default ];
          };
          homeModuleOptions = { lib, ... }: {
            config._module.args.pkgs = pkgs;
            options.home.file = lib.mkOption {
              type = lib.types.attrsOf (lib.types.submodule ({ ... }: {
                options.text = lib.mkOption {
                  type = lib.types.lines;
                  default = "";
                };
              }));
              default = { };
            };
            options.home.packages = lib.mkOption {
              type = lib.types.listOf lib.types.package;
              default = [ ];
            };
            options.systemd.user.services = lib.mkOption {
              type = lib.types.attrsOf (lib.types.submodule {
                options = {
                  Unit = lib.mkOption {
                    type = lib.types.attrs;
                    default = { };
                  };
                  Service = lib.mkOption {
                    type = lib.types.attrs;
                    default = { };
                  };
                  Install = lib.mkOption {
                    type = lib.types.attrs;
                    default = { };
                  };
                };
              });
              default = { };
            };
          };
          enabledHome = nixpkgs.lib.evalModules {
            modules = [
              homeModuleOptions
              self.homeManagerModules.default
              {
                programs.omnitiler.tray.enable = true;
                programs.omnitiler.planner.enable = true;
              }
            ];
          };
          disabledHome = nixpkgs.lib.evalModules {
            modules = [ homeModuleOptions self.homeManagerModules.default ];
          };
          plannerDisabledHome = nixpkgs.lib.evalModules {
            modules = [
              homeModuleOptions
              self.homeManagerModules.default
              { programs.omnitiler.planner.enable = false; }
            ];
          };
          customPlanner = pkgs.runCommand "custom-planner-test" { } ''
            mkdir -p "$out/bin"
            touch "$out/bin/omnitiler"
            chmod +x "$out/bin/omnitiler"
          '';
          customPlannerHome = nixpkgs.lib.evalModules {
            modules = [
              homeModuleOptions
              self.homeManagerModules.default
              {
                programs.omnitiler.planner.enable = true;
                programs.omnitiler.planner.package = customPlanner;
              }
            ];
          };
          activation = enabledNixos.config.environment.etc."xdg/kwinrc".text;
          desktopFile = ".config/autostart/omnitiler.desktop";
          trayUnit = enabledHome.config.systemd.user.services."omnitiler-tray";
          descriptorTemplate = builtins.readFile ./nix/com.omnitiler.Planner.service;
          expectedTemplate = "[D-BUS Service]\nName=com.omnitiler.Planner\nExec=@out@/bin/omnitiler planner-service\nSystemdService=omnitiler-planner.service\n";
          expectedDescriptor = "[D-BUS Service]\nName=com.omnitiler.Planner\nExec=${tray}/bin/omnitiler planner-service\nSystemdService=omnitiler-planner.service\n";
          plannerUnit = enabledHome.config.systemd.user.services."omnitiler-planner";
          defaultPlannerUnit = disabledHome.config.systemd.user.services."omnitiler-planner";
          customPlannerUnit = customPlannerHome.config.systemd.user.services."omnitiler-planner";
        in
        assert activation == "[Plugins]\nomnitiler-kwinEnabled=true\n";
        assert !(nixpkgs.lib.hasInfix "omnitiler-active-borderEnabled" activation);
        assert !(nixpkgs.lib.hasInfix "Planner" activation);
        assert !(nixpkgs.lib.hasInfix "planner" activation);
        assert builtins.elem kwinScript enabledNixos.config.environment.systemPackages;
        assert builtins.elem nativeEffect enabledNixos.config.environment.systemPackages;
        assert builtins.elem nativeSettings enabledNixos.config.environment.systemPackages;
        assert !(builtins.elem tray enabledNixos.config.environment.systemPackages);
        assert !(builtins.hasAttr "xdg/kwinrc" disabledNixos.config.environment.etc);
        assert !(builtins.hasAttr desktopFile enabledHome.config.home.file);
        assert !(builtins.hasAttr desktopFile disabledHome.config.home.file);
        assert !(builtins.hasAttr "activation" enabledHome.config.home);
        assert !(builtins.hasAttr ".config/autostart/omnitiler-planner.desktop" enabledHome.config.home.file);
        assert !(builtins.hasAttr ".config/autostart/omnitiler-planner.desktop" disabledHome.config.home.file);
        assert builtins.hasAttr "omnitiler-tray" enabledHome.config.systemd.user.services;
        assert !(builtins.hasAttr "omnitiler-tray" disabledHome.config.systemd.user.services);
        assert trayUnit.Unit.PartOf == [ "graphical-session.target" ];
        assert trayUnit.Service.ExecStart == "${tray}/bin/omnitiler tray";
        assert trayUnit.Service.Restart == "on-failure";
        assert trayUnit.Install.WantedBy == [ "graphical-session.target" ];
        assert descriptorTemplate == expectedTemplate;
        assert !(nixpkgs.lib.hasInfix "/nix/store" descriptorTemplate);
        assert builtins.replaceStrings [ "@out@" ] [ "${tray}" ] descriptorTemplate == expectedDescriptor;
        assert !(nixpkgs.lib.hasInfix (toString ./.) expectedDescriptor);
        assert disabledHome.config.programs.omnitiler.planner.enable == true;
        assert enabledHome.config.programs.omnitiler.planner.enable == true;
        assert plannerDisabledHome.config.programs.omnitiler.planner.enable == false;
        assert builtins.hasAttr "omnitiler-planner" enabledHome.config.systemd.user.services;
        assert builtins.hasAttr "omnitiler-planner" disabledHome.config.systemd.user.services;
        assert !(builtins.hasAttr "omnitiler-planner" plannerDisabledHome.config.systemd.user.services);
        assert plannerUnit.Service.Type == "dbus";
        assert plannerUnit.Service.BusName == "com.omnitiler.Planner";
        assert plannerUnit.Service.ExecStart == "${tray}/bin/omnitiler planner-service";
        assert plannerUnit.Service.Restart == "no";
        assert builtins.attrNames plannerUnit.Service == [ "BusName" "ExecStart" "Restart" "StandardError" "StandardOutput" "Type" ];
        assert !(builtins.hasAttr "ProtectSystem" plannerUnit.Service);
        assert !(builtins.hasAttr "ProtectHome" plannerUnit.Service);
        assert !(builtins.hasAttr "PrivateDevices" plannerUnit.Service);
        assert !(builtins.hasAttr "PrivateNetwork" plannerUnit.Service);
        assert !(builtins.hasAttr "ProtectKernelTunables" plannerUnit.Service);
        assert !(nixpkgs.lib.hasInfix "/bin/sh" plannerUnit.Service.ExecStart);
        assert !(nixpkgs.lib.hasInfix "sh -c" plannerUnit.Service.ExecStart);
        assert !(nixpkgs.lib.hasInfix (toString ./.) plannerUnit.Service.ExecStart);
        assert nixpkgs.lib.hasInfix "/bin/omnitiler planner-service" plannerUnit.Service.ExecStart;
        assert plannerUnit.Unit.Description == "OmniTiler Planner (on-demand D-Bus service)";
        assert plannerUnit.Install == { };
        assert defaultPlannerUnit.Service.ExecStart == "${tray}/bin/omnitiler planner-service";
        assert builtins.elem tray enabledHome.config.home.packages;
        assert builtins.elem tray disabledHome.config.home.packages;
        assert plannerDisabledHome.config.home.packages == [ ];
        assert customPlannerUnit.Service.ExecStart == "${customPlanner}/bin/omnitiler planner-service";
        assert builtins.elem customPlanner customPlannerHome.config.home.packages;
        assert !(builtins.elem tray customPlannerHome.config.home.packages);
        assert customPlannerUnit.Service.Type == "dbus";
        assert customPlannerUnit.Service.BusName == "com.omnitiler.Planner";
        assert customPlannerUnit.Service.Restart == "no";
        {
          module-boundary = pkgs.runCommand "omnitiler-module-boundary" { } ''
            touch "$out"
          '';
          native-effect = nativeEffect;
          native-settings = nativeSettings;
          # Test-enabled full native build: compiles the effect plus KCM
          # sources with BUILD_TESTING=ON and runs the original hermetic
          # 32-test CTest suite (reconciler, KCM shortcut/config with FFI,
          # metadata validation, Rust FFI) offscreen. Same inputs as the
          # delivery derivation; the KCMs here are test-only artifacts
          # (the shipping effect derivation stays effect-only).
          native-effect-tests = self.lib.mkNativeEffect { inherit pkgs; withTests = true; };
          # Test-enabled settings-only build: compiles the KCM sources with
          # BUILD_TESTING=ON and runs the hermetic settings CTest gates
          # (reconciler, KCM shortcut/config, script discovery validation)
          # offscreen. Effect-gated tests are configured out here.
          native-settings-tests = self.lib.mkNativeSettings { inherit pkgs; withTests = true; };
        });

      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          kwinScript = mkKwinScript {
            inherit pkgs;
            sourceRev = self.rev or "local-dev";
          };
          tray = mkTray {
            inherit pkgs;
            sourceRev = self.rev or "local-dev";
          };
        in
        {
          default = tray;
          kwin-script = kwinScript;
          native-effect = mkNativeEffect { inherit pkgs; };
          native-settings = mkNativeSettings { inherit pkgs; };
          tray = tray;
        });
    };
}
