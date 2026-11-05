# Security Policy

## Supported versions

Security fixes are provided for the latest release only. Please update to the newest version from the
[Releases](../../releases) page before reporting an issue.

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Report it privately using GitHub's private vulnerability reporting:
go to the **Security** tab of this repository and click **Report a vulnerability**.

Include, if you can:
- what you found and why it is a security problem
- steps or a sample file that reproduces it
- the app version and your operating system

You can expect an acknowledgement within about 7 days. This is a small, volunteer-run project, so fix
times vary, but valid reports are taken seriously and you will be credited in the release notes if you wish.

## What this app does (and doesn't) do

- It reads photos and videos from the folder you open, and writes only to a `Favorites` subfolder inside it.
- Deleting a file moves it to the Recycle Bin / Trash. The app never permanently deletes files.
- Rotation and zoom change only the on-screen view, never your files.
- It does not collect telemetry and does not intentionally make network connections.

## Verifying a download

Each release includes a `SHA256SUMS.txt` file. To check a download:

```bash
# Linux / macOS
sha256sum -c SHA256SUMS.txt --ignore-missing

# Windows (PowerShell): compare the output with the matching line in SHA256SUMS.txt
Get-FileHash .\media_viewer-<version>-windows-x86_64.zip -Algorithm SHA256
```

Release files also carry a build attestation proving they were built by this repository's workflow.
With the [GitHub CLI](https://cli.github.com/) installed:

```bash
gh attestation verify <downloaded-file> --repo shua-shua-jo/media-viewer
```

## Known caveats

- Release builds are not code-signed yet, so Windows SmartScreen and macOS Gatekeeper may show a warning
  the first time you run the app.
- The app decodes image files using third-party libraries. Keeping the app updated is the best protection
  against malformed-file bugs.