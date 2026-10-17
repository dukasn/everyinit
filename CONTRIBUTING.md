# Contributing

Source code contributions to Evin and ESH are accepted under the repository's
GPL-3.0-only license. Documentation prose is accepted under CC BY-SA 4.0, and
reusable code examples in documentation are accepted under MIT, as described
in the [documentation licensing notice](docs/contributing/30.licensing.md).
By submitting a contribution, you certify that you have the right to submit it
under the applicable license and agree to the Developer Certificate of Origin
1.1 in [DCO.txt](DCO.txt).

Sign off each commit with `git commit -s`. The sign-off records your
certification for that commit; it does not change the project's license.

## Sign commits

Commits proposed for `main` and `release` must be cryptographically signed;
the planned branch rulesets will enforce this requirement. Sign each commit
with a key you control and register its public key with your Git hosting
account. Never send a private key to a maintainer or put a personal signing
key in repository or Actions secrets. The DCO sign-off and cryptographic
signature are separate requirements; a signed commit still needs
`git commit -s`.

GitHub supports SSH commit signing. A dedicated key is recommended so it can
be managed separately from SSH authentication:

```sh
ssh-keygen -t ed25519 -C "github commit signing" -f ~/.ssh/id_ed25519_signing
git config --global gpg.format ssh
git config --global user.signingkey ~/.ssh/id_ed25519_signing.pub
git config --global commit.gpgsign true
```

Add the contents of `~/.ssh/id_ed25519_signing.pub` to GitHub under **Settings
→ SSH and GPG keys → Signing keys**. Keep the private key on your device and
protect it with a passphrase. Make a signed, DCO-signed test commit and confirm
GitHub marks it **Verified** before opening a PR. GitHub's SSH signature
verification requires Git 2.34 or newer. These steps configure commit
signatures; release artifact signing uses a separate project-controlled key.

For the development workflow, see
[docs/contributing/10-development.md](docs/contributing/10-development.md).
Documentation is licensed under CC BY-SA 4.0 as described in
[docs/contributing/30.licensing.md](docs/contributing/30.licensing.md).
