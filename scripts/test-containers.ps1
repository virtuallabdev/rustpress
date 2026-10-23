param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$project = 'rustpress-test-' + [Guid]::NewGuid().ToString('N').Substring(0, 12)
$network = $project + '-network'
$db = $project + '-db'
$api = $project + '-api'
$admin = $project + '-admin'
$theme = $project + '-theme'
$password = [Guid]::NewGuid().ToString('N') + [Guid]::NewGuid().ToString('N')
$created = [System.Collections.Generic.List[string]]::new()
$networkCreated = $false

function Invoke-Docker {
    param([string[]]$DockerArgs)
    $result = & wsl --exec docker @DockerArgs
    if ($LASTEXITCODE -ne 0) { throw "Docker operation failed (exit $LASTEXITCODE)" }
    return $result
}
function Start-TestContainer {
    param([string]$Name, [string[]]$Options)
    $null = Invoke-Docker -DockerArgs (@('run', '-d', '--name', $Name, '--network', $network) + $Options)
    $created.Add($Name)
}
function Wait-Http {
    param([string]$Url)
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Uri $Url -TimeoutSec 3
            if ($response.StatusCode -eq 200) { return $response }
        } catch [System.Net.WebException] {
            if ($attempt -eq 119) { throw }
        }
        Start-Sleep -Milliseconds 500
    }
    throw "Service not ready: $Url"
}

try {
    $null = Invoke-Docker -DockerArgs @('network', 'create', $network)
    $networkCreated = $true
    Start-TestContainer -Name $db -Options @(
        '-e', 'POSTGRES_DB=rustpress', '-e', 'POSTGRES_USER=rustpress',
        '-e', "POSTGRES_PASSWORD=$password",
        'postgres:18.6-bookworm@sha256:bbf83594798e144bff365a9c191c5d779f181fb3135902064ab8ff11918020fc'
    )
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        & wsl --exec docker exec $db pg_isready -U rustpress -d rustpress *> $null
        if ($LASTEXITCODE -eq 0) { break }
        if ($attempt -eq 119) { throw 'Container PostgreSQL did not become ready' }
        Start-Sleep -Milliseconds 500
    }
    Start-TestContainer -Name $api -Options @(
        '--read-only', '--tmpfs', '/tmp', '--tmpfs', '/data/media:uid=10001,gid=10001',
        '--cap-drop=ALL', '--security-opt=no-new-privileges:true',
        '-e', "DATABASE_URL=postgres://rustpress:$password@$db`:5432/rustpress",
        '-e', 'RUSTPRESS_ENV=development',
        '-e', 'ADMIN_ORIGIN=http://localhost:3001', '-e', 'SITE_ORIGIN=http://localhost:3000',
        'rustpress-api:verify'
    )
    foreach ($entry in @(@($admin, 'admin'), @($theme, 'theme'))) {
        Start-TestContainer -Name $entry[0] -Options @(
            '--read-only', '--tmpfs', '/tmp', '--cap-drop=ALL', '--security-opt=no-new-privileges:true',
            '-p', '127.0.0.1::3000', '-e', "NUXT_API_INTERNAL=http://$api`:8080",
            '-e', 'NUXT_PUBLIC_SITE_ORIGIN=http://localhost:3000', "rustpress-$($entry[1]):verify"
        )
    }
    $adminPort = (Invoke-Docker -DockerArgs @('port', $admin, '3000/tcp')).Trim().Split(':')[-1]
    $themePort = (Invoke-Docker -DockerArgs @('port', $theme, '3000/tcp')).Trim().Split(':')[-1]
    $adminResponse = Wait-Http -Url "http://127.0.0.1:$adminPort/"
    $themeResponse = Wait-Http -Url "http://127.0.0.1:$themePort/en"
    if ($themeResponse.Content -notmatch 'Articles and pages') { throw 'English SSR interface not translated' }
    $providers = Wait-Http -Url "http://127.0.0.1:$adminPort/api/v1/auth/providers"
    if ($providers.Content.Trim() -ne '[]') { throw 'Unexpected OAuth provider configuration' }
    $null = Wait-Http -Url "http://127.0.0.1:$themePort/api/v1/public/contents?locale=en"
    $tables = Invoke-Docker -DockerArgs @('exec', $db, 'psql', '-U', 'rustpress', '-d', 'rustpress', '-tAc',
        "SELECT count(*) FROM information_schema.columns WHERE table_name='contents' AND column_name IN ('locale','markdown','translation_group')")
    if ($tables.Trim() -ne '3') { throw 'Translation/Markdown migration not applied' }
    $apiUser = Invoke-Docker -DockerArgs @('inspect', '--format', '{{.Config.User}}', $api)
    if ($apiUser.Trim() -ne '10001:10001') { throw 'API is not configured to run as the expected non-root user' }
    Write-Output 'Container smoke passed: PostgreSQL 18.6, non-root read-only API/admin/theme, migrations, proxies and English SSR.'
} finally {
    foreach ($name in $created) {
        & wsl --exec docker rm -f -v $name *> $null
        if ($LASTEXITCODE -ne 0) { Write-Warning "Could not remove test container $name" }
    }
    if ($networkCreated) {
        & wsl --exec docker network rm $network *> $null
        if ($LASTEXITCODE -ne 0) { Write-Warning "Could not remove test network $network" }
    }
}
