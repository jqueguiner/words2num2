# AUR package: `python-words2num2`

Files in this directory mirror what needs to live in the AUR git repo at
`ssh://aur@aur.archlinux.org/python-words2num2.git`.

## One-time AUR setup

1. Create an AUR account: https://aur.archlinux.org/register
2. Add your SSH public key to that account (Account → My Account → Edit
   → SSH Public Key).
3. Verify SSH access: `ssh aur@aur.archlinux.org help` should print a
   help banner.

## First-time publish

```bash
# Clone the empty AUR repo for the new package name. The first push
# creates it.
git clone ssh://aur@aur.archlinux.org/python-words2num2.git
cd python-words2num2

# Copy PKGBUILD + .SRCINFO from this directory.
cp /path/to/words2num2/packaging/aur/python-words2num2/PKGBUILD .
cp /path/to/words2num2/packaging/aur/python-words2num2/.SRCINFO .

# Sanity-check locally on an Arch box (or makepkg in a container).
makepkg --syncdeps --clean
namcap PKGBUILD
namcap python-words2num2-0.2.1-1-any.pkg.tar.zst

git add PKGBUILD .SRCINFO
git commit -m "Initial import: python-words2num2 0.2.1"
git push origin master
```

The package will appear at
https://aur.archlinux.org/packages/python-words2num2 within a minute.

## Updating to a new release (manual)

When a new words2num2 version ships to PyPI:

1. Bump `pkgver` in `PKGBUILD` (and reset `pkgrel=1`).
2. Refresh the sha256:
   ```bash
   curl -sL https://files.pythonhosted.org/packages/source/w/words2num2/words2num2-${NEWVER}.tar.gz \
     | sha256sum
   ```
   Paste the value into `sha256sums=(...)`.
3. Regenerate `.SRCINFO`:
   ```bash
   makepkg --printsrcinfo > .SRCINFO
   ```
4. Commit + push:
   ```bash
   git commit -am "Bump to ${NEWVER}"
   git push
   ```

## Automated release bumps

The `aur-publish.yml` GitHub Actions workflow auto-bumps the AUR
package when a new tag (`v*`) is pushed:

1. Waits for the matching sdist to appear on PyPI.
2. Computes its sha256.
3. Patches `PKGBUILD` and regenerates `.SRCINFO` inside an Arch Docker
   container.
4. Pushes to `ssh://aur@aur.archlinux.org/python-words2num2.git`.

To enable, add a repo secret `AUR_SSH_PRIVATE_KEY` containing the
private SSH key whose public half is registered on the maintainer's
AUR account.

## Installing on Arch / Manjaro

After the package is in AUR:

```bash
# With an AUR helper:
yay -S python-words2num2
paru -S python-words2num2

# Without a helper:
git clone https://aur.archlinux.org/python-words2num2.git
cd python-words2num2
makepkg -si
```
