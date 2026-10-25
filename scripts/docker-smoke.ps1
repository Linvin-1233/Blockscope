param(
    [string]$Tag = "blockscope:local"
)

$ErrorActionPreference = "Stop"

docker run --rm --cpus=1 --memory=1g $Tag capacity --json
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

'{"type":"shutdown"}' | docker run --rm -i --cpus=1 --memory=1g $Tag serve
exit $LASTEXITCODE
