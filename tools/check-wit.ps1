# Verifies that the published `pumpkin-plugin-api` crate that this build
# resolves carries the plugin WIT of the server release it has to load on.
#
# A green build does not catch a WIT change: the component still compiles, but
# the server refuses to load it with a bare "type-checking export func
# 'handle-event'" and no explanation. Pumpkin also kept the
# `0.2.0+26.3-26.51` version string across a WIT change, so the version
# string cannot be used to check this either.
#
# The version is read from the `pumpkin-plugin-api` requirement in the root
# `Cargo.toml`, so there is only one place to bump. The expected WIT tree is
# the tree hash of `crates/pumpkin-plugin-wit/v0.1` in the matching Pumpkin
# release, passed in as $env:WIT_TREE.

$ErrorActionPreference = 'Stop'

# The requirement can live in the root manifest or in a crate manifest, so
# every manifest in the workspace is searched.
$manifests = Get-ChildItem $env:GITHUB_WORKSPACE -Recurse -Filter 'Cargo.toml' |
	Where-Object { $_.FullName -notlike '*\target\*' }
$version = $null
foreach ($manifest in $manifests) {
	$requirement = Select-String -Path $manifest.FullName `
		-Pattern 'pumpkin-plugin-api\s*=\s*"=?([0-9][^"]*)"?' |
		Select-Object -First 1
	if ($requirement -ne $null) {
		$version = $requirement.Matches[0].Groups[1].Value
		break
	}
}
if ($version -eq $null) {
	throw "no pumpkin-plugin-api requirement in any manifest under $env:GITHUB_WORKSPACE"
}
$expected = $env:WIT_TREE
if ([string]::IsNullOrWhiteSpace($expected)) {
	throw "WIT_TREE is not set"
}
"pumpkin-plugin-api $version"

$work = Join-Path $env:RUNNER_TEMP 'wit-check'
Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# The crate comes from crates.io, exactly like the build resolves it.
$crate = Join-Path $work 'api.crate'
Invoke-WebRequest `
	"https://crates.io/api/v1/crates/pumpkin-plugin-api/$version/download" `
	-OutFile $crate
tar -xzf $crate -C $work

$wit = Join-Path $work "pumpkin-plugin-api-$version/wit"
if (!(Test-Path $wit)) {
	throw "the published crate has no wit/ directory"
}

# The tree hash is the number to compare, because it is the same number the
# Pumpkin checkout gives. Git hashes the WIT files the way it hashes every
# other file, so they are put into a scratch repository under the same path
# the release uses.
$repo = Join-Path $work 'repo'
$v01 = Join-Path $repo 'crates/pumpkin-plugin-wit/v0.1'
New-Item -ItemType Directory -Force $v01 | Out-Null
Copy-Item "$wit/*" $v01

Push-Location $repo
try {
	git init -q .
	git config core.autocrlf false
	git add -A
	$tree = git rev-parse "$(git write-tree):crates/pumpkin-plugin-wit/v0.1"
} finally {
	Pop-Location
}

"WIT tree: $tree"
"expected: $expected"
if ($tree -ne $expected) {
	throw "the published pumpkin-plugin-api $version has WIT tree $tree, not $expected. A component built against it will not load on the matching server release; update the requirement and WIT_TREE deliberately, not automatically."
}
"WIT matches."

# The release notes name the API and the WIT tree, so they are passed on to
# the workflow that needs them.
if (![string]::IsNullOrWhiteSpace($env:GITHUB_OUTPUT)) {
	"api=$version" >> $env:GITHUB_OUTPUT
	"wit=$tree" >> $env:GITHUB_OUTPUT
}
