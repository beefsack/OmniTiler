cmake_minimum_required(VERSION 3.19)

project(omnitiler-native-effect LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Split build: the two settings KCMs are KWin-independent (Qt/KF6 only,
# D-Bus effect reconfigure tolerates an absent effect), while the
# active-border effect is ABI-bound to KWin headers plus the Cargo
# staticlib. Both default ON to keep the existing full developer build;
# distro/Nix settings-only consumers configure with
# -DOMNITILER_BUILD_EFFECT=OFF (needs only ECM 6.24 + Qt6/KF6),
# and effect-only companions with
# -DOMNITILER_BUILD_SETTINGS=OFF. The ECM floor is 6.24 everywhere (the
# lowest tested extra-cmake-modules) and the effect requires KWin 6.6+
# headers; the KWin script itself feature-detects the 6.7-only per-output
# workspace API at runtime and degrades to shared on 6.6.
option(OMNITILER_BUILD_EFFECT "Build the ABI-bound KWin effect" ON)
option(OMNITILER_BUILD_SETTINGS "Build the KWin-independent settings KCMs" ON)

find_package(ECM 6.24.0 REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})

include(KDEInstallDirs)
include(KDECMakeSettings)
if(OMNITILER_BUILD_EFFECT)
    include(ECMSetupQtPluginMacroNames)
endif()

find_package(Qt6 REQUIRED COMPONENTS Core DBus Gui Widgets)

if(OMNITILER_BUILD_EFFECT)
    find_package(KWin 6.6.0 REQUIRED)
endif()
find_package(KF6CoreAddons REQUIRED)
find_package(KF6Config REQUIRED)
if(OMNITILER_BUILD_EFFECT)
    find_package(KF6ColorScheme REQUIRED)
endif()
find_package(KF6I18n REQUIRED)
find_package(KF6KCMUtils REQUIRED)
find_package(KF6WidgetsAddons REQUIRED)
include("${KF6I18n_DIR}/KF6I18nMacros.cmake")

if(OMNITILER_BUILD_EFFECT)
    ecm_setup_qtplugin_macro_names(
        MACRO_NAMES
            KWIN_EFFECT_FACTORY
            KWIN_EFFECT_FACTORY_ENABLED
            KWIN_EFFECT_FACTORY_SUPPORTED
            KWIN_EFFECT_FACTORY_SUPPORTED_ENABLED
    )
endif()

set(config_SOURCES)
kconfig_add_kcfg_files(config_SOURCES activeborderconfig.kcfgc)

# AR10 effect Rust: a Cargo workspace staticlib
# (crates/tiler-kwin-effect-ffi, group highlight policy plus the slice 1
# drag oracle) built by a plain custom command invoking Cargo. No Corrosion,
# no new system dependency: cargo ships with the existing devenv Rust
# toolchain. The crate reuses serde and tiler-core gates, replacing the
# bare-rustc build and the hand-written group JSON parser. Rust keeps group
# visibility and drag verdict policy behind the same POD C ABI.
# find_program/find_package stay before all consumers so linkage never
# relies on transitive libs. Effect-only: skipped for settings-only builds,
# which must not find KWin/Cargo/effect dependencies.
if(OMNITILER_BUILD_EFFECT)
find_program(CARGO_PROGRAM cargo REQUIRED)
# The Rust staticlib references pthread/dl (std thread internals, backtrace
# symbolization). Link them explicitly and portably so the shared plugins
# do not rely on transitive linkage.
find_package(Threads REQUIRED)

# Workspace layout: this file lives at kwin/native-effect under the
# workspace root. Nix/flake wires the workspace inputs via its source
# fileset; a missing workspace fails this command closed.
set(EFFECT_WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
set(EFFECT_FFI_CRATE_DIR "${EFFECT_WORKSPACE_ROOT}/crates/tiler-kwin-effect-ffi")
set(EFFECT_CARGO_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/cargo-effect")
set(EFFECT_STATICLIB "${EFFECT_CARGO_TARGET_DIR}/release/libtiler_kwin_effect_ffi.a")
# Nix offline vendor: when CARGO_NET_OFFLINE=true (set by the Nix
# mkNativeEffect derivation alongside its CARGO_HOME vendor config), pass
# --offline so Cargo fails closed instead of hitting the network. Host
# builds leave the env unset and keep the default locked (network-capable)
# behavior with no channel/target change.
if(DEFINED ENV{CARGO_NET_OFFLINE} AND "$ENV{CARGO_NET_OFFLINE}" STREQUAL "true")
    set(EFFECT_CARGO_OFFLINE_ARG --offline)
else()
    set(EFFECT_CARGO_OFFLINE_ARG "")
endif()
# Release-archive offline vendor: when OMNITILER_VENDOR_DIR points at
# the source archive's vendor/ directory (absolute path), pass cargo --config
# source replacement so the workspace resolves crates-io from that directory
# with no network and no CARGO_HOME setup. Distro recipes set this alongside
# CARGO_NET_OFFLINE=true; unset keeps host/Nix behavior unchanged.
if(DEFINED ENV{OMNITILER_VENDOR_DIR} AND NOT "$ENV{OMNITILER_VENDOR_DIR}" STREQUAL "")
    set(EFFECT_CARGO_VENDOR_ARGS
        --config source.crates-io.replace-with="vendored-sources"
        --config source.vendored-sources.directory="$ENV{OMNITILER_VENDOR_DIR}")
else()
    set(EFFECT_CARGO_VENDOR_ARGS "")
endif()
add_custom_command(
    OUTPUT "${EFFECT_STATICLIB}"
    COMMAND "${CARGO_PROGRAM}"
        build
        --release
        --locked
        ${EFFECT_CARGO_OFFLINE_ARG}
        ${EFFECT_CARGO_VENDOR_ARGS}
        --manifest-path "${EFFECT_FFI_CRATE_DIR}/Cargo.toml"
        -p tiler-kwin-effect-ffi
        --target-dir "${EFFECT_CARGO_TARGET_DIR}"
    DEPENDS
        "${EFFECT_WORKSPACE_ROOT}/Cargo.toml"
        "${EFFECT_WORKSPACE_ROOT}/Cargo.lock"
        "${EFFECT_FFI_CRATE_DIR}/Cargo.toml"
        "${EFFECT_FFI_CRATE_DIR}/src/lib.rs"
        "${EFFECT_FFI_CRATE_DIR}/src/group_highlight.rs"
        "${EFFECT_FFI_CRATE_DIR}/src/drag_oracle.rs"
        "${EFFECT_FFI_CRATE_DIR}/src/visual.rs"
        "${EFFECT_WORKSPACE_ROOT}/crates/tiler-core/Cargo.toml"
        "${EFFECT_WORKSPACE_ROOT}/crates/tiler-core/src/lib.rs"
        "${EFFECT_WORKSPACE_ROOT}/crates/tiler-core/src/visual.rs"
        "${EFFECT_WORKSPACE_ROOT}/crates/tiler-core/src/ids.rs"
        "${EFFECT_WORKSPACE_ROOT}/crates/tiler-core/src/bounds.rs"
        "${EFFECT_WORKSPACE_ROOT}/crates/tiler-core/src/geometry.rs"
    COMMENT "Building tiler-kwin-effect-ffi staticlib with Cargo"
    VERBATIM
)
add_custom_target(omnitiler-effect-ffi-rs DEPENDS "${EFFECT_STATICLIB}")
endif()

if(OMNITILER_BUILD_EFFECT)
kcoreaddons_add_plugin(omnitiler-active-border
    INSTALL_NAMESPACE "kwin/effects/plugins"
    SOURCES
        activewindowborder.h
        activewindowborder.cpp
        drag_oracle_ffi.h
        group_highlight_ffi.h
        visual_policy_ffi.h
        ${config_SOURCES}
)
add_dependencies(omnitiler-active-border omnitiler-effect-ffi-rs)
target_link_libraries(omnitiler-active-border PRIVATE
    "${EFFECT_STATICLIB}"
    Threads::Threads
    ${CMAKE_DL_LIBS}
    KWin::kwin
    KF6::ConfigCore
    KF6::ConfigGui
    KF6::ColorScheme
    Qt6::Core
    Qt6::Gui
    Qt6::DBus
)
target_compile_options(omnitiler-active-border PRIVATE -Wall -Wextra -Werror)
endif()

if(OMNITILER_BUILD_SETTINGS)
set(unified_kcm_SOURCES
    unifiedsettings_module.cpp
    unifiedsettings_module.h
    shortcutreconciler.cpp
    shortcutreconciler.h
    ${config_SOURCES}
)
ki18n_wrap_ui(unified_kcm_SOURCES unifiedsettings.ui)

kcoreaddons_add_plugin(omnitiler-active-border_config
    INSTALL_NAMESPACE "kwin/effects/configs"
    SOURCES
        activeborderconfig_module.cpp
        activeborderconfig_module.h
        ${unified_kcm_SOURCES}
)
target_compile_definitions(omnitiler-active-border_config PRIVATE -DTRANSLATION_DOMAIN="kwin")
target_link_libraries(omnitiler-active-border_config PRIVATE
    KF6::CoreAddons
    KF6::ConfigCore
    KF6::ConfigGui
    KF6::I18n
    KF6::KCMUtils
    KF6::WidgetsAddons
    Qt6::Core
    Qt6::DBus
    Qt6::Widgets
)
target_compile_options(omnitiler-active-border_config PRIVATE -Wall -Wextra -Werror)

# Unified settings page: both existing plugin IDs/namespace entries display
# the identical shared QWidget page via two thin factories. Script settings
# (workspace mode, tiling gaps) live on the same page; no separate script
# page internals remain.
kcoreaddons_add_plugin(omnitiler-kwin_config
    INSTALL_NAMESPACE "kwin/scripts/configs"
    SOURCES
        scriptconfig_module.cpp
        scriptconfig_module.h
        ${unified_kcm_SOURCES}
)
target_compile_definitions(omnitiler-kwin_config PRIVATE -DTRANSLATION_DOMAIN="kwin")
target_link_libraries(omnitiler-kwin_config PRIVATE
    KF6::CoreAddons
    KF6::ConfigCore
    KF6::ConfigGui
    KF6::I18n
    KF6::KCMUtils
    KF6::WidgetsAddons
    Qt6::Core
    Qt6::DBus
    Qt6::Widgets
)
target_compile_options(omnitiler-kwin_config PRIVATE -Wall -Wextra -Werror)
endif()

include(CTest)

if(BUILD_TESTING)
    if(OMNITILER_BUILD_EFFECT)
    add_executable(omnitiler-active-border-logic-test
        activeborderlogic_test.cpp
    )
    add_dependencies(omnitiler-active-border-logic-test omnitiler-effect-ffi-rs)
    target_link_libraries(omnitiler-active-border-logic-test PRIVATE
        "${EFFECT_STATICLIB}"
        Threads::Threads
        ${CMAKE_DL_LIBS}
        Qt6::Core Qt6::Gui)
    target_compile_options(omnitiler-active-border-logic-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-logic
        COMMAND omnitiler-active-border-logic-test)

    add_executable(omnitiler-active-border-group-test
        activebordergroup_test.cpp
    )
    add_dependencies(omnitiler-active-border-group-test omnitiler-effect-ffi-rs)
    target_link_libraries(omnitiler-active-border-group-test PRIVATE
        "${EFFECT_STATICLIB}"
        Threads::Threads
        ${CMAKE_DL_LIBS}
        Qt6::Core
        Qt6::Gui
    )
    target_compile_options(omnitiler-active-border-group-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-group-highlight
        COMMAND omnitiler-active-border-group-test)

    endif()

    if(OMNITILER_BUILD_SETTINGS)
    set(kcm_config_test_SOURCES
        activeborderconfig_module_test.cpp
        activeborderconfig_module.cpp
        unifiedsettings_module.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(kcm_config_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-active-border-kcm-config-test
        ${kcm_config_test_SOURCES}
    )
    if(OMNITILER_BUILD_EFFECT)
        add_dependencies(omnitiler-active-border-kcm-config-test omnitiler-effect-ffi-rs)
    endif()
    target_link_libraries(omnitiler-active-border-kcm-config-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-active-border-kcm-config-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ActiveBorderConfigModuleFactory
    )
    if(OMNITILER_BUILD_EFFECT)
        target_link_libraries(omnitiler-active-border-kcm-config-test PRIVATE
            "${EFFECT_STATICLIB}"
            Threads::Threads
            ${CMAKE_DL_LIBS}
        )
        target_compile_definitions(omnitiler-active-border-kcm-config-test PRIVATE
            -DOMNITILER_HAVE_EFFECT_FFI=1
        )
    endif()
    target_compile_options(omnitiler-active-border-kcm-config-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-kcm-dbus
        COMMAND omnitiler-active-border-kcm-config-test dbus)
    add_test(NAME native-effect-kcm-hotapply
        COMMAND omnitiler-active-border-kcm-config-test hotapply)
    add_test(NAME native-effect-kcm-border
        COMMAND omnitiler-active-border-kcm-config-test border)
    add_test(NAME native-effect-kcm-config-reload
        COMMAND omnitiler-active-border-kcm-config-test config)
    set_tests_properties(
        native-effect-kcm-dbus
        native-effect-kcm-hotapply
        native-effect-kcm-border
        native-effect-kcm-config-reload
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    set(kcm_shortcut_test_SOURCES
        activeborderconfig_shortcut_test.cpp
        activeborderconfig_module.cpp
        unifiedsettings_module.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(kcm_shortcut_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-active-border-kcm-shortcut-test
        ${kcm_shortcut_test_SOURCES}
    )
    target_link_libraries(omnitiler-active-border-kcm-shortcut-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-active-border-kcm-shortcut-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ActiveBorderConfigModuleFactory
    )
    target_compile_options(omnitiler-active-border-kcm-shortcut-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-kcm-shortcut-ordinary
        COMMAND omnitiler-active-border-kcm-shortcut-test ordinary)
    add_test(NAME native-effect-kcm-shortcut-recovery
        COMMAND omnitiler-active-border-kcm-shortcut-test recovery)
    add_test(NAME native-effect-kcm-shortcut-confirm
        COMMAND omnitiler-active-border-kcm-shortcut-test confirm)
    add_test(NAME native-effect-kcm-shortcut-state
        COMMAND omnitiler-active-border-kcm-shortcut-test state)
    add_test(NAME native-effect-kcm-shortcut-force
        COMMAND omnitiler-active-border-kcm-shortcut-test force)
    add_test(NAME native-effect-kcm-shortcut-migration
        COMMAND omnitiler-active-border-kcm-shortcut-test migration)
    add_test(NAME native-effect-kcm-shortcut-selection
        COMMAND omnitiler-active-border-kcm-shortcut-test selection)
    set_tests_properties(
        native-effect-kcm-shortcut-ordinary
        native-effect-kcm-shortcut-recovery
        native-effect-kcm-shortcut-confirm
        native-effect-kcm-shortcut-state
        native-effect-kcm-shortcut-force
        native-effect-kcm-shortcut-migration
        native-effect-kcm-shortcut-selection
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    set(script_config_test_SOURCES
        scriptconfig_module_test.cpp
        scriptconfig_module.cpp
        unifiedsettings_module.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(script_config_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-script-config-test
        ${script_config_test_SOURCES}
    )
    target_link_libraries(omnitiler-script-config-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-script-config-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ScriptConfigModuleFactory
    )
    target_compile_options(omnitiler-script-config-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-script-config-dbus
        COMMAND omnitiler-script-config-test dbus)
    add_test(NAME native-script-config-gaps
        COMMAND omnitiler-script-config-test gaps)
    add_test(NAME native-script-config-save
        COMMAND omnitiler-script-config-test save)
    add_test(NAME native-script-config-windows
        COMMAND omnitiler-script-config-test windows)
    set_tests_properties(
        native-script-config-dbus
        native-script-config-gaps
        native-script-config-save
        native-script-config-windows
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    add_test(NAME native-script-config-discovery-validation
        COMMAND ${CMAKE_COMMAND}
            -DSCRIPT_METADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/../metadata.json
            -DSCRIPT_KCM_METADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/scriptconfig_module.json
            -DSCRIPT_KCM_SOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/scriptconfig_module.cpp
            -DSCRIPT_CMAKE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists.txt
            -DEXPECTED_SCRIPT_CONFIG_REFERENCE=kwin/scripts/configs/omnitiler-kwin_config
            -DEXPECTED_SCRIPT_KCM_ID=omnitiler-kwin_config
            -P ${CMAKE_CURRENT_SOURCE_DIR}/validate-scriptconfig.cmake)

    add_executable(omnitiler-shortcut-reconciler-test
        shortcutreconciler_test.cpp
        shortcutreconciler.cpp
    )
    target_link_libraries(omnitiler-shortcut-reconciler-test PRIVATE
        KF6::ConfigCore
        Qt6::Core
        Qt6::DBus
        Qt6::Gui
    )
    target_compile_options(omnitiler-shortcut-reconciler-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-shortcut-success
        COMMAND omnitiler-shortcut-reconciler-test success)
    add_test(NAME native-effect-shortcut-conflict
        COMMAND omnitiler-shortcut-reconciler-test conflict)
    add_test(NAME native-effect-shortcut-malformed
        COMMAND omnitiler-shortcut-reconciler-test malformed)
    add_test(NAME native-effect-shortcut-owner
        COMMAND omnitiler-shortcut-reconciler-test owner)
    add_test(NAME native-effect-shortcut-recovery
        COMMAND omnitiler-shortcut-reconciler-test recovery)
    add_test(NAME native-effect-shortcut-external
        COMMAND omnitiler-shortcut-reconciler-test external)
    add_test(NAME native-effect-shortcut-journal
        COMMAND omnitiler-shortcut-reconciler-test journal)
    add_test(NAME native-effect-shortcut-force
        COMMAND omnitiler-shortcut-reconciler-test force)
    add_test(NAME native-effect-shortcut-diag
        COMMAND omnitiler-shortcut-reconciler-test diag)
    add_test(NAME native-effect-shortcut-selection
        COMMAND omnitiler-shortcut-reconciler-test selection)
    set_tests_properties(
        native-effect-shortcut-success
        native-effect-shortcut-conflict
        native-effect-shortcut-malformed
        native-effect-shortcut-owner
        native-effect-shortcut-recovery
        native-effect-shortcut-external
        native-effect-shortcut-journal
        native-effect-shortcut-force
        native-effect-shortcut-diag
        native-effect-shortcut-selection
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    endif()

    if(OMNITILER_BUILD_EFFECT)
    add_test(NAME native-effect-metadata-factory-validation
        COMMAND ${CMAKE_COMMAND}
            -DMETADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/metadata.json
            -DSOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DKCM_METADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activeborderconfig_module.json
            -DKCM_SOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activeborderconfig_module.cpp
            -DGROUP_HEADER=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.h
            -DGROUP_IMPL=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DGROUP_LOGIC=${CMAKE_CURRENT_SOURCE_DIR}/activeborderlogic.h
            -DGROUP_TEST=${CMAKE_CURRENT_SOURCE_DIR}/activebordergroup_test.cpp
            -DGROUP_FFI=${CMAKE_CURRENT_SOURCE_DIR}/group_highlight_ffi.h
            -DGROUP_RUST=${EFFECT_FFI_CRATE_DIR}/src/group_highlight.rs
            -DDRAG_HEADER=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.h
            -DDRAG_IMPL=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DDRAG_FFI=${CMAKE_CURRENT_SOURCE_DIR}/drag_oracle_ffi.h
            -DDRAG_RUST=${EFFECT_FFI_CRATE_DIR}/src/drag_oracle.rs
            -DEXPECTED_PLUGIN_ID=omnitiler-active-border
            -P ${CMAKE_CURRENT_SOURCE_DIR}/validate-metadata.cmake)

    add_test(NAME native-effect-unified-lifecycle
        COMMAND ${CMAKE_COMMAND}
            -DSOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DHEADER_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.h
            -DCMAKE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists.txt
            -P ${CMAKE_CURRENT_SOURCE_DIR}/validate-unified-lifecycle.cmake)

    # Effect Rust unit tests run through Cargo against the same workspace
    # staticlib sources, filtered per policy so each CTest keeps its focused
    # scope (replaces the retired rustc --test binaries).
    add_test(NAME native-effect-drag-oracle-rs
        COMMAND "${CARGO_PROGRAM}" test --release --locked
            ${EFFECT_CARGO_OFFLINE_ARG}
            ${EFFECT_CARGO_VENDOR_ARGS}
            --manifest-path "${EFFECT_FFI_CRATE_DIR}/Cargo.toml"
            -p tiler-kwin-effect-ffi --lib drag_oracle::
            --target-dir "${EFFECT_CARGO_TARGET_DIR}")

    add_test(NAME native-effect-group-highlight-rs
        COMMAND "${CARGO_PROGRAM}" test --release --locked
            ${EFFECT_CARGO_OFFLINE_ARG}
            ${EFFECT_CARGO_VENDOR_ARGS}
            --manifest-path "${EFFECT_FFI_CRATE_DIR}/Cargo.toml"
            -p tiler-kwin-effect-ffi --lib group_highlight::
            --target-dir "${EFFECT_CARGO_TARGET_DIR}")
    endif()
endif()
