/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  isCloudflareWebSocketProxyUrl,
  normalizeSessionProxyUrl,
  sessionProxyPartitionForUrl,
} from "./proxyUrl";

test("normalizes scheme-less session proxy host:port values as http proxies", () => {
  assert.equal(normalizeSessionProxyUrl("localhost:8080"), "http://localhost:8080");
  assert.equal(normalizeSessionProxyUrl("proxy.example.com:8080"), "http://proxy.example.com:8080");
  assert.equal(normalizeSessionProxyUrl("127.0.0.1:8080"), "http://127.0.0.1:8080");
});

test("accepts supported explicit session proxy schemes", () => {
  assert.equal(normalizeSessionProxyUrl("socks5://proxy.example.com:1080"), "socks5://proxy.example.com:1080");
});

test("normalizes Cloudflare WebSocket proxy URLs with the default TLS port", () => {
  const proxyUrl = normalizeSessionProxyUrl("wss://onproxy.example.com");
  assert.equal(proxyUrl, "wss://onproxy.example.com:443");
  assert.equal(isCloudflareWebSocketProxyUrl(proxyUrl!), true);
});

test("fills standard default ports when a proxy URL omits one", () => {
  assert.equal(normalizeSessionProxyUrl("https://proxy.example.com"), "https://proxy.example.com:443");
  assert.equal(normalizeSessionProxyUrl("http://proxy.example.com"), "http://proxy.example.com:80");
});

test("rejects unsupported explicit session proxy schemes", () => {
  assert.throws(
    () => normalizeSessionProxyUrl("ftp://proxy.example.com:21"),
    /Invalid session proxy URL/,
  );
});

test("derives stable opaque proxy session partitions", () => {
  const withCredentials = sessionProxyPartitionForUrl("http://user:secret@proxy.example.com:8080");
  const sameProxy = sessionProxyPartitionForUrl("http://user:secret@proxy.example.com:8080");
  const differentProxy = sessionProxyPartitionForUrl("http://other.example.com:8080");

  assert.equal(withCredentials, sameProxy);
  assert.notEqual(withCredentials, differentProxy);
  assert.match(withCredentials, /^opennow:gfn-session-proxy:[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
});
