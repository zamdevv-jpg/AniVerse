const PAGE_CACHE = "aniverse-offline-pages-v1";
const VIDEO_DOWNLOAD_CACHE = "aniverse-video-downloads-v1";
const SHELL_CACHE = "aniverse-shell-v23";
const SHELL_ASSETS = [
  "/static/css/style.css?v=20261110-player-v14",
  "/static/js/app.js?v=20261110-player-v12",
  "/static/js/supabase.js?v=20261110-player-v5",
  "/static/js/logo.png",
  "/static/favicon.svg",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(SHELL_CACHE).then((cache) => cache.addAll(SHELL_ASSETS).catch(() => {})),
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key.startsWith("aniverse-") && ![PAGE_CACHE, VIDEO_DOWNLOAD_CACHE, SHELL_CACHE].includes(key))
          .map((key) => caches.delete(key)),
      ),
    ),
  );
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin === self.location.origin && url.pathname.startsWith("/__device-downloads/")) {
    event.respondWith(
      caches.open(VIDEO_DOWNLOAD_CACHE).then(async (cache) => {
        const saved = await cache.match(request);
        return saved || new Response("Video ini tidak ada di penyimpanan perangkat.", {
          status: 404,
          headers: { "Content-Type": "text/plain; charset=utf-8" },
        });
      }),
    );
    return;
  }

  if (request.mode === "navigate" && url.origin === self.location.origin) {
    event.respondWith(
      fetch(request)
        .then(async (response) => {
          const saved = await caches.match(request);
          if (response.ok && saved) {
            const cache = await caches.open(PAGE_CACHE);
            await cache.put(request, response.clone());
          }
          if (!response.ok && saved) return saved;
          return response;
        })
        .catch(async () => {
          const saved = await caches.match(request);
          if (saved) return saved;
          return new Response(
            "<meta charset='utf-8'><meta name='viewport' content='width=device-width'><title>Offline</title><body style='font:16px system-ui;background:#111;color:#eee;padding:2rem'><h1>Sedang offline</h1><p>Buka anime yang sudah kamu simpan offline atau sambungkan kembali ke internet.</p><a href='/offline' style='color:#ffd23f'>Lihat koleksi offline</a></body>",
            { headers: { "Content-Type": "text/html; charset=utf-8" }, status: 503 },
          );
        }),
    );
    return;
  }

  if (url.origin === self.location.origin && url.pathname.startsWith("/static/")) {
    event.respondWith(
      caches.match(request).then((cached) => {
        if (cached) return cached;
        return fetch(request).then(async (response) => {
          if (response.ok) {
            const cache = await caches.open(SHELL_CACHE);
            await cache.put(request, response.clone());
          }
          return response;
        });
      }),
    );
    return;
  }

  if (request.destination === "image") {
    event.respondWith(
      caches.match(request).then((cached) => {
        if (cached) return cached;
        return fetch(request).then(async (response) => {
          if (response.ok || response.type === "opaque") {
            const cache = await caches.open(SHELL_CACHE);
            await cache.put(request, response.clone());
          }
          return response;
        });
      }),
    );
  }
});
