{ pkgs }:
let
  scripts = {
    start = pkgs.writeShellScriptBin "start" ''
      set -e
      echo "Checking services..."
      ra-start "$@"
    '';

    stop = pkgs.writeShellScriptBin "stop" ''
      set -e
      echo "Stopping services..."
      ra-stop "$@"
      echo "Done."
    '';

    status = pkgs.writeShellScriptBin "status" ''
      set -e
      echo "Service Status:"
      echo ""
      ra-status "$@"
    '';
  };
in
builtins.attrValues scripts
