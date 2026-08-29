{
  description = "eutheto reproducible development and build environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config.allowUnfree = false;
        };
    in
    {
      formatter = forAllSystems (system: (pkgsFor system).nixfmt);

      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          src = (import ./nix/source-filter.nix { inherit (pkgs) lib; }) ./.;
          tooling = import ./nix/tooling.nix { inherit pkgs system; };
        in
        import ./nix/packages.nix {
          inherit
            pkgs
            src
            tooling
            ;
        }
      );

      legacyPackages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          ortools-worker-contract = import ./nix/ortools-worker.nix { inherit pkgs; };
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          src = (import ./nix/source-filter.nix { inherit (pkgs) lib; }) ./.;
        in
        import ./nix/checks.nix { inherit pkgs src; }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          tooling = import ./nix/tooling.nix { inherit pkgs system; };
          mkDevShell =
            shellName: extraPackages:
            import ./nix/dev-shell.nix {
              inherit
                pkgs
                tooling
                shellName
                extraPackages
                ;
            };
        in
        {
          default = mkDevShell "default" [ ];
          full = mkDevShell "full" tooling.full;
          release = import ./nix/release-shell.nix { inherit pkgs tooling; };
        }
      );
    };
}
