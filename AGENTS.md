Prefer `nix develop` commands generally and maintain the nix flake developer shell.

The invariants to abide are located in ./Vault invariants.md

Be very strict regarding error handling. Everything must be handled, Ask about all edge cases.

Abide strict TDD. Write tests in batches interact with me so I can ensure that you are testing for the correct behaviours.

Prefer tests that acutally run the built binary. You should produce temp files programatically when doing so.

Prefer many small, focused tests that test a single behaviour.

Do not write developer-only facing code-comments.

Generally prefer idiomatic rust unless it means excessive use of type gymnastics. Do not fear refactoring old code. No code is too much to refactor.
