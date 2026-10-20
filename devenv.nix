{ pkgs, lib, config, inputs, ... }:

{
  languages.rust.enable = true;
  languages.javascript.enable = true;
  languages.javascript.package = pkgs.nodejs_24;

  packages = with pkgs; [
    cargo-about
    cargo-deny
    clang-tools
    dbus
    gh
    jq
    just
    license_finder
    python3
    systemd
    unzip
    zip
    kdePackages.kpackage
    weston
    # Runtime display libraries for the dev-only tiler-live-fixture helper.
    # The helper compiles without X11/Wayland headers (dynamic-loading Cargo
    # features) but dlopens these at runtime for live use.
    wayland
    libxkbcommon
    xorg.libX11
    xorg.libXcursor
    xorg.libXext
    xorg.libXi
    xorg.libXrandr
    xorg.libxcb
  ];

  # Make the runtime display libraries above findable by dlopen. A refreshed
  # shell (session restart) is required before live use.
  env.LD_LIBRARY_PATH = lib.makeLibraryPath (with pkgs; [
    wayland
    libxkbcommon
    xorg.libX11
    xorg.libXcursor
    xorg.libXext
    xorg.libXi
    xorg.libXrandr
    xorg.libxcb
  ]);

  enterShell = ''
    export PATH=${pkgs.clang-tools}/bin:$PATH
  '';
}
