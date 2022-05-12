$nf = 3000
$frames = Get-ChildItem -Path test\_raw

for ($i = 0; $i -lt $nf; $i++) {
    $next = $frames[$i % $frames.length]
    Write-Output "Writing frame $i ($next)"

    $dst = "$i".PadLeft(4, "0")
    Copy-Item -Path $next -Destination "test\$dst.png"
}