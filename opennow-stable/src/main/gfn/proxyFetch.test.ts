/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { normalizeSessionProxyUrl, sessionProxyPartitionForUrl } from "./proxyUrl";

test("normalizes scheme-less session proxy host:port values as http proxies", () => {
  assert.equal(normalizeSessionProxyUrl("localhost:8080"), "http://localhost:8080");
  assert.equal(normalizeSessionProxyUrl("proxy.example.com:8080"), "http://proxy.example.com:8080");
  assert.equal(normalizeSessionProxyUrl("127.0.0.1:8080"), "http://127.0.0.1:8080");
});

test("accepts supported explicit session proxy schemes", () => {
  assert.equal(normalizeSessionProxyUrl("socks5://proxy.example.com:1080"), "socks5://proxy.example.com:1080");
});

test("rejects unsupported explicit session proxy schemes", () => {
  assert.throws(
    () => normalizeSessionProxyUrl("ftp://proxy.example.com:21"),
    /Invalid session proxy URL/,
  );
});

test("derives stable redacted proxy session partitions", () => {
  const withCredentials = sessionProxyPartitionForUrl("http://user:secret@proxy.example.com:8080");
  const sameProxy = sessionProxyPartitionForUrl("http://user:secret@proxy.example.com:8080");
  const differentProxy = sessionProxyPartitionForUrl("http://other.example.com:8080");

  assert.equal(withCredentials, sameProxy);
  assert.notEqual(withCredentials, differentProxy);
  assert.match(withCredentials, /^opennow:gfn-session-proxy:[a-f0-9]{32}$/);
  assert.equal(withCredentials.includes("proxy.example.com"), false);
  assert.equal(withCredentials.includes("secret"), false);
});
