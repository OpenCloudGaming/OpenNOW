/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveSrtpSaltHex,
  extractAdvertisedSrtpProfileFromHeaders,
  extractAdvertisedSrtpProfileFromSdp,
} from "./srtp";

test("extractAdvertisedSrtpProfileFromSdp reads standard crypto suites", () => {
  assert.equal(
    extractAdvertisedSrtpProfileFromSdp([
      "v=0",
      "m=video 0 RTP/SAVP 96",
      "a=crypto:1 AEAD_AES_256_GCM inline:ignored",
      "",
    ].join("\r\n")),
    "AEAD_AES_256_GCM",
  );
});

test("extractAdvertisedSrtpProfileFromSdp accepts an explicit SRTP-named attribute", () => {
  assert.equal(
    extractAdvertisedSrtpProfileFromSdp(
      "v=0\r\na=x-provider-srtp-suite:AES_CM_128_HMAC_SHA1_80\r\n",
    ),
    "AES_CM_128_HMAC_SHA1_80",
  );
});

test("extractAdvertisedSrtpProfileFromHeaders reads explicit SETUP transport data", () => {
  assert.equal(
    extractAdvertisedSrtpProfileFromHeaders({
      transport: "RTP/SAVP;unicast;profile=AEAD_AES_128_GCM;X-GS-ServerPort=5004",
    }),
    "AEAD_AES_128_GCM",
  );
  assert.equal(
    extractAdvertisedSrtpProfileFromHeaders({
      "x-provider-srtp-suite": "AES_CM_128_HMAC_SHA1_32",
    }),
    "AES_CM_128_HMAC_SHA1_32",
  );
});

test("profile parser ignores unscoped and unknown profile text", () => {
  assert.equal(
    extractAdvertisedSrtpProfileFromSdp(
      "v=0\r\na=x-note:prefer AEAD_AES_256_GCM\r\n",
    ),
    null,
  );
  assert.equal(
    extractAdvertisedSrtpProfileFromSdp(
      "v=0\r\na=x-provider-srtp-suite:UNSUPPORTED_PROFILE\r\n",
    ),
    null,
  );
  assert.equal(
    extractAdvertisedSrtpProfileFromSdp(
      "v=0\r\na=x-provider-srtp-supported-profiles:AEAD_AES_128_GCM AEAD_AES_256_GCM\r\n",
    ),
    null,
  );
  assert.equal(
    extractAdvertisedSrtpProfileFromHeaders({
      "x-note": "AEAD_AES_256_GCM",
    }),
    null,
  );
});

test("deriveSrtpSaltHex returns the direct 12-byte key-id salt", () => {
  assert.equal(deriveSrtpSaltHex(2664076126), "00000000000000009ECA935E");
});
