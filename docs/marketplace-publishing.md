# Publishing PumpkinPIE to the Pumpkin Market

The Market listing is updated by a GitHub Action from `release.yml`, in the same
run that publishes the `.wasm` as a GitHub release. One tag push does both.

Why an action: the Market's publish endpoint is `PUT /api/plugins/{id}` with a
multipart body (`wasm` part + `metadata` JSON part), authenticated with
`Authorization: Bearer <token>`. It works, but it has no public documentation and
no CLI, so the request has to be hand-built — and Windows PowerShell 5.1 has no
`-Form` parameter, which means assembling the multipart body byte by byte.
`filiphsps/pumpkin-plugins/actions/publish-to-pumpkin-market` does all of it.

## The step

```yaml
- name: Publish to the Pumpkin Market
  uses: filiphsps/pumpkin-plugins/actions/publish-to-pumpkin-market@publish-to-pumpkin-market-v0.0.4
  with:
    plugin-id: TV3Ord4H
    version: ${{ steps.version.outputs.value }}
    wasm-file: target/wasm32-wasip2/release/pumpkin_pie_plugin.wasm
    api-token: ${{ secrets.MARKET_API_TOKEN }}
    track: stable
    release-notes: |
      Built against ... at ...
```

Set exactly one of `plugin-id` or `plugin-name`, never both.

**`plugin-id` is used, not `plugin-name`, and that is deliberate.** The listing is
named `PumpkinPIE  | Placeholders for all` — two spaces and a pipe. An exact-name
match would break if anybody tidies the title, and the action's lookup requires a
case-insensitive *exact* match. The public ID is stable.

`plugin-id` accepts either the numeric database ID (`77`) or the public ID
(`TV3Ord4H`).

## Release notes

`release-notes` goes into the Market version metadata, so write it for someone
reading the Market page rather than for a git log.

## The token

Repository secret `MARKET_API_TOKEN`, created with:

```powershell
gh secret set MARKET_API_TOKEN --repo Rennex07/PumpkinPIE
```

The token needs the `plugins:update` and `plugins:versions:upload` scopes.

**Rotate the token after it has been used.** A marketplace token pasted into a
chat conversation is a token that is in that conversation's history, so treat any
credential that has passed through one as compromised.

## Cutting a release

```powershell
# from C:\dBackup\personal\work\PumpkinPAPI
git add -A
git commit -m "Release 0.2.4"
git push origin main

git tag -a v0.2.4 -m "0.2.4"
git push origin v0.2.4
```

The tag push triggers `release.yml`, which builds the component against the
pinned Pumpkin revision, publishes the GitHub release, then updates the Market.
Watch it with `gh run watch --repo Rennex07/PumpkinPIE`.

If the Market step fails, the GitHub release still exists — the two are
independent steps.

## Reference

The action does not build the plugin and does not create a listing; it only
uploads a file to one that already exists. Reference and the full input list:
<https://filiphsps.github.io/pumpkin-plugins/api/actions/publish-to-pumpkin-market/>
