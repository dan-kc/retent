Prefer `nix develop` commands generally and maintain the nix flake developer shell.

The invariants to abide are located in ./Vault invariants.md

Be very strict regarding error handling. Everything must be handled and it should never panic.

Abide strict TDD. Write tests in batches and confirm approval before proceeding on. Prefer tests that acutally run the built binary. You should produce temp files programatically when doing so. Prefer small focused tests.

Do not write developer-only facing code-comments.

Generally prefer functional code.
