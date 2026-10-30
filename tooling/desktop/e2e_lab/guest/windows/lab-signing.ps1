# Isolated laboratory trust only. Private keys remain non-exportable in this guest.
function Assert-LabCertificate($Certificate) {
  if ($Certificate.Subject -ne 'CN=Sniptale isolated E2E' -or $Certificate.Issuer -ne $Certificate.Subject -or
      $Certificate.NotBefore -gt (Get-Date) -or $Certificate.NotAfter -le (Get-Date) -or
      @($Certificate.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' }).Count -ne 1) {
    throw "invalid isolated laboratory certificate"
  }
}
function Trust-LabCertificate($Certificate) {
  Assert-LabCertificate $Certificate
  $public = [Security.Cryptography.X509Certificates.X509Certificate2]::new($Certificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
  foreach ($pair in @(@('Root', 'LocalMachine'), @('TrustedPublisher', 'CurrentUser'))) {
    $store = [Security.Cryptography.X509Certificates.X509Store]::new($pair[0], $pair[1])
    try {
      $store.Open('ReadOnly')
      $present = @($store.Certificates | Where-Object { [Convert]::ToBase64String($_.RawData) -ceq [Convert]::ToBase64String($public.RawData) }).Count -eq 1
      if (-not $present) { $store.Close(); $store.Open('ReadWrite'); $store.Add($public) }
    } finally { $store.Close() }
  }
}
function New-LabSigningIdentity {
  $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object {
    $_.Subject -eq 'CN=Sniptale isolated E2E' -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date).AddDays(1)
  } | Sort-Object NotAfter -Descending | Select-Object -First 1
  if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Sniptale isolated E2E' -HashAlgorithm SHA256 -KeyAlgorithm RSA -KeyLength 2048 -KeyExportPolicy NonExportable -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddDays(14)
  }
  Trust-LabCertificate $cert
  return $cert
}
function Import-LabSigningIdentity([string] $Path, [string] $ExpectedSha256) {
  if ($ExpectedSha256 -notmatch '^[0-9a-f]{64}$' -or (Get-Item -LiteralPath $Path).Length -gt 16384 -or
      (Get-FileHash -LiteralPath $Path).Hash.ToLowerInvariant() -ne $ExpectedSha256) { throw "laboratory certificate provenance mismatch" }
  $cert = [Security.Cryptography.X509Certificates.X509Certificate2]::new($Path)
  Trust-LabCertificate $cert
  return $cert
}
function Sign-LabArtifact([string] $Path, $Certificate) {
  $result = Set-AuthenticodeSignature -FilePath $Path -Certificate $Certificate -HashAlgorithm SHA256
  if ($result.Status -ne 'Valid') { throw "laboratory artifact signing failed" }
  $actual = Get-AuthenticodeSignature -LiteralPath $Path
  if ($actual.Status -ne 'Valid' -or [Convert]::ToBase64String($actual.SignerCertificate.RawData) -cne [Convert]::ToBase64String($Certificate.RawData)) { throw "laboratory signature verification failed" }
}
