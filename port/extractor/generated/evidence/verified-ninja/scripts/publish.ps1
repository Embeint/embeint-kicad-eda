$ErrorActionPreference = 'Stop'
$ninjaPublicationScripts = 'C:/Users/KarlVonRichter/.t3/phase0-verified-ninja'
wsl -d Ubuntu -- mkdir /home/karlvonrichter/kicad-port.build-lock
if ($LASTEXITCODE -ne 0) { throw 'Shared lock is held; publication checks were not started.' }
try {
    python "$ninjaPublicationScripts/adopt.py"
    if ($LASTEXITCODE -ne 0) { throw 'Artifact adoption failed.' }
    python "$ninjaPublicationScripts/checks.py"
    if ($LASTEXITCODE -ne 0) { throw 'Windows artifact and policy checks failed.' }
    python "$ninjaPublicationScripts/publish_docs.py"
    if ($LASTEXITCODE -ne 0) { throw 'Maintained provenance documentation generation failed.' }
}
finally {
    wsl -d Ubuntu -- rmdir /home/karlvonrichter/kicad-port.build-lock
}
