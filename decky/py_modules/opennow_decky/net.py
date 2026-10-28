import json
import ssl
import urllib.request
from typing import Any
from urllib.parse import urlparse

USER_AGENT = "OpenNOW-Decky"


class HostError(ValueError):
    pass


def ssl_context() -> ssl.SSLContext:
    try:
        import certifi
        return ssl.create_default_context(cafile=certifi.where())
    except ImportError:
        return ssl.create_default_context()


def require_host(url: str, hosts: tuple[str, ...]) -> None:
    parsed = urlparse(url)
    if (parsed.scheme != "https" or parsed.hostname not in hosts or parsed.username is not None
            or parsed.password is not None or parsed.port not in (None, 443)):
        raise HostError(f"Refusing to contact {parsed.hostname or url}")


class AllowedRedirects(urllib.request.HTTPRedirectHandler):
    max_redirections = 5

    def __init__(self, hosts: tuple[str, ...]):
        self.hosts = hosts

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        require_host(newurl, self.hosts)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def open_url(url: str, body: dict | None = None, headers: dict | None = None, timeout: float = 30,
             hosts: tuple[str, ...] | None = None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(url, data=data, headers={
        "User-Agent": USER_AGENT,
        **({"Content-Type": "application/json"} if body is not None else {}),
        **(headers or {}),
    })
    if hosts is None:
        return urllib.request.urlopen(request, timeout=timeout, context=ssl_context())
    require_host(url, hosts)
    opener = urllib.request.build_opener(AllowedRedirects(hosts), urllib.request.HTTPSHandler(context=ssl_context()))
    return opener.open(request, timeout=timeout)


def http_json(url: str, body: dict | None = None, headers: dict | None = None, timeout: float = 30,
              hosts: tuple[str, ...] | None = None) -> Any:
    with open_url(url, body, {"Accept": "application/json", **(headers or {})}, timeout, hosts=hosts) as response:
        return json.load(response)
