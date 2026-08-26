import assert from "node:assert/strict";
import test from "node:test";

import {
  isZoneHostname,
  normalizeTrustedCloudMatchBaseUrl,
  resolvePollStopBase,
  resolveSessionControlBaseUrl,
  selectCreateSessionBase,
} from "./cloudmatchTransport";

test("isZoneHostname accepts NVIDIA CloudMatch domains and their subdomains", () => {
  assert.equal(isZoneHostname("cloudmatch.nvidiagrid.net"), true);
  assert.equal(isZoneHostname("np-ams-06.cloudmatchbeta.nvidiagrid.net"), true);
  assert.equal(isZoneHostname("NP-AMS-06.CLOUDMATCHBETA.NVIDIAGRID.NET."), true);
});

test("isZoneHostname rejects hostnames that only contain a CloudMatch domain substring", () => {
  assert.equal(isZoneHostname("cloudmatch.nvidiagrid.net.attacker.example"), false);
  assert.equal(isZoneHostname("attacker-cloudmatchbeta.nvidiagrid.net"), false);
  assert.equal(isZoneHostname("cloudmatchbeta.nvidiagrid.net.evil.test"), false);
});

test("session control base follows official zone-LB poll host", () => {
  assert.equal(
    resolveSessionControlBaseUrl(
      "np-ams-06.cloudmatchbeta.nvidiagrid.net",
      "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
    ),
    "https://np-ams-06.cloudmatchbeta.nvidiagrid.net",
  );
  assert.equal(
    resolveSessionControlBaseUrl("203.0.113.10", "https://np-lax-01.cloudmatchbeta.nvidiagrid.net"),
    "https://np-lax-01.cloudmatchbeta.nvidiagrid.net",
  );
});

test("create host prefers regional metro URL over zone LB when available", () => {
  assert.equal(
    selectCreateSessionBase([
      "https://np-frk-08.cloudmatchbeta.nvidiagrid.net",
      "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
      "https://np-ams-06.cloudmatchbeta.nvidiagrid.net",
    ]),
    "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
  );
  assert.equal(
    selectCreateSessionBase(["https://np-lax-01.cloudmatchbeta.nvidiagrid.net"]),
    "https://np-lax-01.cloudmatchbeta.nvidiagrid.net",
  );
});

test("poll/stop base uses assigned CloudMatch zone host or real seat IP", () => {
  assert.equal(
    resolvePollStopBase(
      "prod",
      "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
      "np-ams-06.cloudmatchbeta.nvidiagrid.net",
    ),
    "https://np-ams-06.cloudmatchbeta.nvidiagrid.net",
  );
  assert.equal(
    resolvePollStopBase("prod", "https://np-lax-01.cloudmatchbeta.nvidiagrid.net", "203.0.113.10"),
    "https://203.0.113.10",
  );
});

test("trusted CloudMatch endpoints require a clean NVIDIA HTTPS origin", () => {
  assert.equal(
    normalizeTrustedCloudMatchBaseUrl("https://NP-AMS-06.CLOUDMATCHBETA.NVIDIAGRID.NET./"),
    "https://np-ams-06.cloudmatchbeta.nvidiagrid.net",
  );
  assert.equal(
    normalizeTrustedCloudMatchBaseUrl("https://prod.bpc.geforcenow.nvidiagrid.net:443/"),
    "https://prod.bpc.geforcenow.nvidiagrid.net",
  );

  for (const endpoint of [
    "http://prod.cloudmatchbeta.nvidiagrid.net/",
    "https://localhost/",
    "https://127.0.0.1/",
    "https://cloudmatchbeta.nvidiagrid.net.attacker.example/",
    "https://attacker.example/?next=nvidiagrid.net",
    "https://user:password@prod.cloudmatchbeta.nvidiagrid.net/",
    "https://prod.cloudmatchbeta.nvidiagrid.net:8443/",
    "https://prod.cloudmatchbeta.nvidiagrid.net/redirect",
  ]) {
    assert.throws(() => normalizeTrustedCloudMatchBaseUrl(endpoint), /Untrusted CloudMatch endpoint/);
  }
});
