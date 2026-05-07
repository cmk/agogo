---
name: pii
day: fri
paths: [.]
cadence: monthly
---
You are scanning agogo for PII or secrets that slipped past the
pre-commit `scripts/check_pii.sh` gate. The hook scans staged diffs;
historical content or bypassed commits can still leak.

Read first:
- scripts/check_pii.sh.
- .pii-allow, if present.

Mechanical pass:

1. Absolute home-directory paths.
   Pattern: `/Users/<name>/` or `/home/<name>/` outside allowlisted
   examples. Must-fix unless explicitly allowed.

2. Common API token / key shapes.
   Patterns include:
   - `sk-[A-Za-z0-9_-]{20,}`
   - `glpat-[A-Za-z0-9_-]{20,}`
   - `ghp_[A-Za-z0-9]{36}`
   - `xox[baprs]-[A-Za-z0-9-]{10,}`
   - long hex/base64 strings near "token", "key", "secret",
     "password", or "credential"
   Any plausible real token is must-fix and should recommend rotation.

3. Email addresses.
   Allowed maintainer addresses:
   - `chris.mckinlay@gmail.com`
   - `6869885+cmk@users.noreply.github.com`
   Flag other personal emails unless they are clearly public project
   metadata.

4. Internal hostnames/IPs.
   Pattern: `*.internal`, `*.corp`, `*.local`, or RFC1918 addresses
   outside test examples. Follow-up unless credentials or private
   infrastructure details are exposed.

Output format:
- One section per category that has findings.
- Use exact severity labels `[must-fix]` and `[follow-up]`.
- For must-fix findings, include recommended remediation.
- If there are zero findings, output exactly:
  `no findings`

Anti-themes:
- Clearly fake example tokens are not findings.
- GitHub noreply maintainer addresses and public package metadata are
  allowed.
- Paths inside historical review logs may mention public GitHub URLs;
  do not flag those as PII.
