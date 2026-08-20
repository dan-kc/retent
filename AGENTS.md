Prefer `nix develop` commands generally and maintain the nix flake developer shell.

The invariants to abide are located in ./Vault invariants.md

Be very strict regarding error handling. Everything must be handled, Ask about all edge cases.

Abide strict TDD. Write tests in batches and confirm approval before proceeding on. Confirm approval after implementation too. Prefer tests that acutally run the built binary. You should produce temp files programatically when doing so. Prefer small focused tests.

Do not write developer-only facing code-comments.

Generally prefer idiomatic rust. Do not prefer it if it means lots of type gymnastics.
