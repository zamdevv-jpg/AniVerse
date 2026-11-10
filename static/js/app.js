/* =========================================================
   AniVerse — interaksi klien
   Bookmark, riwayat, kualitas otomatis, dan koleksi lokal.
   ========================================================= */
(() => {
  "use strict";

  const STORE = {
    bookmarks: "otakunime:bookmarks",
    history: "otakunime:history",
    downloads: "aniverse:downloads",
    offline: "aniverse:offline-pages",
    quality: "aniverse:quality",
    volume: "aniverse:player-volume",
    audioBoost: "aniverse:player-audio-boost",
    playbackRate: "aniverse:player-playback-rate",
    autoReplay: "aniverse:player-auto-replay",
    autoContinue: "aniverse:player-auto-continue",
  };

  const OFFLINE_PAGE_CACHE = "aniverse-offline-pages-v1";
  const VIDEO_DOWNLOAD_CACHE = "aniverse-video-downloads-v1";
  const DOWNLOAD_HANDLE_DB = "aniverse-download-handles-v1";
  const $ = (selector, scope = document) => scope.querySelector(selector);
  const $$ = (selector, scope = document) => Array.from(scope.querySelectorAll(selector));

  // Sembunyikan menu konteks browser saat klik kanan pada area situs.
  document.addEventListener("contextmenu", (event) => event.preventDefault());

  /* ---------------------------------------------------------
     Penyimpanan lokal
     --------------------------------------------------------- */

  function readStore(key) {
    try {
      const raw = localStorage.getItem(key);
      const parsed = raw ? JSON.parse(raw) : [];
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  function writeStore(key, value) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
      return true;
    } catch {
      return false;
    }
  }

  function readValue(key, fallback = null) {
    try {
      const raw = localStorage.getItem(key);
      return raw ? JSON.parse(raw) : fallback;
    } catch {
      return fallback;
    }
  }

  function writeValue(key, value) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
      return true;
    } catch {
      return false;
    }
  }

  const escapeHtml = (value) =>
    String(value ?? "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");

  function initials(title) {
    const first = String(title ?? "").trim().charAt(0);
    return first ? first.toUpperCase() : "?";
  }

  function relativeTime(timestamp) {
    const minutes = Math.max(1, Math.floor((Date.now() - Number(timestamp || Date.now())) / 60000));
    if (minutes < 60) return `${minutes} menit lalu`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} jam lalu`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days} hari lalu`;
    return new Date(Number(timestamp)).toLocaleDateString("id-ID", { day: "numeric", month: "short" });
  }

  function makeWatchTitleDisplayOnly() {
    const title = $(".watch-copy h1");
    if (title?.querySelector("a")) title.replaceChildren(document.createTextNode(title.textContent || ""));
    const seriesCrumb = $('.breadcrumbs a[href^="/anime/"]');
    if (seriesCrumb) seriesCrumb.replaceWith(document.createTextNode(seriesCrumb.textContent || ""));
  }

  function formatDuration(seconds) {
    const total = Math.max(0, Math.round(Number(seconds) || 0));
    const minutes = Math.floor(total / 60);
    const rest = total % 60;
    return `${minutes}:${String(rest).padStart(2, "0")}`;
  }

  /* ---------------------------------------------------------
     Toast
     --------------------------------------------------------- */

  function toast(message, iconId = "i-check") {
    const stack = $("#toast-stack");
    if (!stack) return;
    const node = document.createElement("div");
    node.className = "toast";
    node.innerHTML = `<svg class="icon" aria-hidden="true"><use href="#${iconId}"/></svg><span>${escapeHtml(message)}</span>`;
    stack.appendChild(node);
    setTimeout(() => {
      node.classList.add("is-leaving");
      setTimeout(() => node.remove(), 240);
    }, 2400);
  }

  /* ---------------------------------------------------------
     Bookmark
     --------------------------------------------------------- */

  function getBookmarks() {
    return readStore(STORE.bookmarks);
  }

  function isBookmarked(slug) {
    return getBookmarks().some((item) => item.slug === slug);
  }

  function saveBookmark(data) {
    const list = getBookmarks().filter((item) => item.slug !== data.slug);
    list.unshift({
      slug: data.slug,
      title: data.title,
      image: data.image || "",
      href: data.href || `/anime/${encodeURIComponent(data.slug)}`,
      savedAt: Date.now(),
    });
    return writeStore(STORE.bookmarks, list.slice(0, 150));
  }

  function removeBookmark(slug) {
    return writeStore(STORE.bookmarks, getBookmarks().filter((item) => item.slug !== slug));
  }

  function toggleBookmark(data) {
    if (!data.slug) return false;
    if (isBookmarked(data.slug)) {
      removeBookmark(data.slug);
      toast("Dihapus dari bookmark", "i-bookmark");
      return false;
    }
    saveBookmark(data);
    toast("Disimpan ke bookmark", "i-bookmark");
    return true;
  }

  function paintBookmarkButtons() {
    $$("[data-bookmark]").forEach((button) => {
      const active = isBookmarked(button.dataset.slug);
      button.classList.toggle("is-bookmarked", active);
      button.setAttribute("aria-pressed", active ? "true" : "false");
      const label = $(".bookmark-label", button);
      if (label) label.textContent = active ? "Tersimpan" : "Bookmark";
      const use = $("use", button);
      if (use) use.setAttribute("href", active ? "#i-bookmark-fill" : "#i-bookmark");
    });
  }

  function bookmarkCardMarkup(item) {
    const image = item.image
      ? `<img src="${escapeHtml(item.image)}" alt="${escapeHtml(item.title)}" loading="lazy" decoding="async" data-fallback="${escapeHtml(initials(item.title))}">`
      : `<span class="poster-fallback" aria-hidden="true">${escapeHtml(initials(item.title))}</span>`;
    return `
      <a class="poster-card" href="${escapeHtml(item.href)}" title="${escapeHtml(item.title)}">
        <div class="poster">
          ${image}
          <span class="poster-shade" aria-hidden="true"></span>
          <div class="poster-top"><span class="chip-format">Bookmark</span></div>
          <span class="poster-play" aria-hidden="true"><svg class="icon fill"><use href="#i-play"/></svg></span>
        </div>
        <div class="poster-info">
          <h3 class="poster-title">${escapeHtml(item.title)}</h3>
          <p class="poster-sub">Disimpan</p>
        </div>
      </a>`;
  }

  function renderBookmarkPage() {
    const grid = $("#bookmark-grid");
    if (!grid) return;
    const empty = $("#bookmark-empty");
    const items = getBookmarks();
    const counter = $("#bookmark-count");
    if (counter) counter.textContent = String(items.length);

    if (items.length === 0) {
      grid.innerHTML = "";
      if (empty) empty.hidden = false;
      return;
    }
    if (empty) empty.hidden = true;
    grid.innerHTML = items.map(bookmarkCardMarkup).join("");
    applyImageFallbacks(grid);
  }

  /* ---------------------------------------------------------
     Riwayat tontonan
     --------------------------------------------------------- */

  function getHistory() {
    return readStore(STORE.history);
  }

  function recordHistory(data) {
    if (!data || !data.slug) return;
    const list = getHistory();
    const existing = list.find((item) => item.slug === data.slug) || {};
    const merged = {
      ...existing,
      slug: data.slug,
      title: data.title || existing.title || "Episode",
      image: data.image || existing.image || "",
      episode: data.episode || existing.episode || "",
      href: data.href || existing.href || `/watch/${encodeURIComponent(data.slug)}`,
      watchedAt: Date.now(),
    };
    const rest = list.filter((item) => item.slug !== data.slug);
    writeStore(STORE.history, [merged, ...rest].slice(0, 60));
  }

  function updateHistoryEntry(slug, patch) {
    if (!slug) return;
    const list = getHistory();
    const index = list.findIndex((item) => item.slug === slug);
    if (index < 0) return;
    list[index] = { ...list[index], ...patch };
    writeStore(STORE.history, list);
  }

  function renderContinueWatching() {
    const section = $("#continue-watching");
    const grid = $("#continue-grid");
    if (!section || !grid) return;

    const items = getHistory().slice(0, 6);
    if (items.length === 0) {
      section.hidden = true;
      return;
    }

    section.hidden = false;
    grid.innerHTML = items
      .map((item) => {
        const image = item.image
          ? `<img src="${escapeHtml(item.image)}" alt="${escapeHtml(item.title)}" loading="lazy" decoding="async" data-fallback="${escapeHtml(initials(item.title))}">`
          : `<span class="poster-fallback" aria-hidden="true">${escapeHtml(initials(item.title))}</span>`;
        const percent = Math.max(0, Math.min(100, Number(item.progress) || 0));
        return `
          <a class="poster-card" href="${escapeHtml(item.href)}" title="${escapeHtml(item.title)}">
            <div class="poster">
              ${image}
              <span class="poster-shade" aria-hidden="true"></span>
              ${item.episode ? `<span class="chip-ep">Eps ${escapeHtml(item.episode)}</span>` : ""}
              ${percent > 0 ? `<span class="history-bar"><i style="width:${percent}%"></i></span>` : ""}
              <span class="poster-play" aria-hidden="true"><svg class="icon fill"><use href="#i-play"/></svg></span>
            </div>
            <div class="poster-info">
              <h3 class="poster-title">${escapeHtml(item.title)}</h3>
              <p class="poster-sub">Lanjut tonton</p>
            </div>
          </a>`;
      })
      .join("");
    applyImageFallbacks(grid);
  }

  function dayLabel(timestamp) {
    const date = new Date(Number(timestamp) || Date.now());
    const today = new Date();
    const startOf = (value) => new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
    const diff = Math.round((startOf(today) - startOf(date)) / 86400000);
    if (diff <= 0) return "Hari ini";
    if (diff === 1) return "Kemarin";
    return date.toLocaleDateString("id-ID", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  }

  function initHistoryPage() {
    const list = $("#history-list");
    if (!list) return;
    const empty = $("#history-empty");

    const paint = () => {
      const items = getHistory();
      if (empty) empty.hidden = items.length > 0;

      if (items.length === 0) {
        list.innerHTML = "";
        return;
      }

      const groups = [];
      items.forEach((item) => {
        const label = dayLabel(item.watchedAt);
        let group = groups.find((entry) => entry.label === label);
        if (!group) {
          group = { label, items: [] };
          groups.push(group);
        }
        group.items.push(item);
      });

      list.innerHTML = groups
        .map(
          (group) => `
        <section class="history-group">
          <h2 class="history-date">${escapeHtml(group.label)}</h2>
          ${group.items
            .map((item) => {
              const percent = Math.max(0, Math.min(100, Number(item.progress) || 0));
              const image = item.image
                ? `<img src="${escapeHtml(item.image)}" alt="" loading="lazy" decoding="async" data-fallback="${escapeHtml(initials(item.title))}">`
                : `<b>${escapeHtml(initials(item.title))}</b>`;
              return `
              <a class="history-row" href="${escapeHtml(item.href)}">
                <span class="history-thumb">
                  ${image}
                  ${percent > 0 ? `<span class="history-bar"><i style="width:${percent}%"></i></span>` : ""}
                </span>
                <span class="history-body">
                  <strong>${escapeHtml(item.title)}</strong>
                  <span>Episode ${escapeHtml(item.episode || "—")} · ${escapeHtml(relativeTime(item.watchedAt))}</span>
                  <small class="history-continue">Lanjutkan menonton <svg class="icon"><use href="#i-chevron-right"/></svg></small>
                </span>
                <svg class="icon history-chevron"><use href="#i-chevron-right"/></svg>
              </a>`;
            })
            .join("")}
        </section>`,
        )
        .join("");
      applyImageFallbacks(list);
    };

    paint();
    $("#clear-history")?.addEventListener("click", () => {
      writeStore(STORE.history, []);
      paint();
      updateAccountStats();
      toast("Riwayat tontonan dihapus", "i-trash");
    });
  }

  /* ---------------------------------------------------------
     Fallback gambar
     --------------------------------------------------------- */

  function applyImageFallbacks(scope = document) {
    $$("img[data-fallback]", scope).forEach((img) => {
      if (img.dataset.fallbackBound === "1") return;
      img.dataset.fallbackBound = "1";
      img.addEventListener(
        "error",
        () => {
          const label = img.dataset.fallback || initials(img.alt);
          const holder = document.createElement("span");
          holder.className = "poster-fallback";
          holder.setAttribute("aria-hidden", "true");
          holder.textContent = label;
          img.replaceWith(holder);
        },
        { once: true },
      );
      if (img.complete && img.naturalWidth === 0) {
        img.dispatchEvent(new Event("error"));
      }
    });
  }

  /* ---------------------------------------------------------
     Header & pencarian langsung
     --------------------------------------------------------- */

  const header = $("#site-header");
  const mobileSearchToggle = $("#mobile-search-toggle");
  const mobileSearchBar = $("#mobile-search-bar");

  if (header) {
    const onScroll = () => header.classList.toggle("is-scrolled", window.scrollY > 8);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
  }

  if (mobileSearchToggle && mobileSearchBar) {
    mobileSearchToggle.addEventListener("click", () => {
      const willShow = mobileSearchBar.hidden;
      mobileSearchBar.hidden = !willShow;
      mobileSearchToggle.setAttribute("aria-expanded", willShow ? "true" : "false");
      if (willShow) {
        const input = $("input", mobileSearchBar);
        if (input) setTimeout(() => input.focus(), 60);
      }
    });
  }

  const searchForm = $(".header-search");
  const searchInput = $("#live-search");
  const searchDropdown = $("#search-dropdown");

  if (searchForm && searchInput && searchDropdown) {
    let controller = null;
    let debounceTimer = 0;
    let lastQuery = "";
    let selectedIndex = -1;
    let items = [];

    const hideDropdown = () => {
      searchDropdown.hidden = true;
      searchDropdown.innerHTML = "";
      selectedIndex = -1;
      items = [];
    };

    const paintSelection = () => {
      $$(".search-item", searchDropdown).forEach((node, index) => {
        node.classList.toggle("is-selected", index === selectedIndex);
        if (index === selectedIndex) node.scrollIntoView({ block: "nearest" });
      });
    };

    const renderResults = (query, results) => {
      items = results;
      if (results.length === 0) {
        searchDropdown.innerHTML = `<div class="search-empty">Tidak ada hasil untuk “${escapeHtml(query)}”.</div>`;
      } else {
        searchDropdown.innerHTML = results
          .map((item, index) => {
            const image = item.image
              ? `<img src="${escapeHtml(item.image)}" alt="" decoding="async">`
              : `<span class="poster-fallback" aria-hidden="true" style="font-size:15px">${escapeHtml(initials(item.title))}</span>`;
            return `
              <a class="search-item" href="${escapeHtml(item.href)}" data-index="${index}">
                <span class="search-item-img">${image}</span>
                <span class="search-item-body">
                  <strong>${escapeHtml(item.title)}</strong>
                  <span>${escapeHtml([item.episode, item.year, item.kind_label].filter(Boolean).join(" · ") || "Anime")}</span>
                </span>
              </a>`;
          })
          .join("");
        searchDropdown.insertAdjacentHTML(
          "beforeend",
          `<a class="search-footer" href="/anime?q=${encodeURIComponent(query)}">Lihat semua hasil untuk “${escapeHtml(query)}”</a>`,
        );
      }
      searchDropdown.hidden = false;
      applyImageFallbacks(searchDropdown);
    };

    const runSearch = async (query) => {
      if (controller) controller.abort();
      controller = new AbortController();
      searchDropdown.hidden = false;
      searchDropdown.innerHTML = `<div class="search-loading">Mencari “${escapeHtml(query)}”…</div>`;

      try {
        const response = await fetch(`/api/search?q=${encodeURIComponent(query)}`, {
          headers: { Accept: "application/json" },
          signal: controller.signal,
        });
        if (!response.ok) throw new Error(String(response.status));
        const payload = await response.json();
        if (searchInput.value.trim() !== query) return;
        renderResults(query, Array.isArray(payload.results) ? payload.results : []);
      } catch (error) {
        if (error.name === "AbortError") return;
        searchDropdown.innerHTML = `<div class="search-empty">Pencarian gagal. Coba lagi.</div>`;
      }
    };

    searchInput.addEventListener("input", () => {
      const query = searchInput.value.trim();
      clearTimeout(debounceTimer);
      selectedIndex = -1;

      if (query.length < 2) {
        hideDropdown();
        lastQuery = "";
        return;
      }
      if (query === lastQuery) return;
      lastQuery = query;
      debounceTimer = window.setTimeout(() => runSearch(query), 320);
    });

    searchInput.addEventListener("focus", () => {
      if (lastQuery.length >= 2 && searchDropdown.hidden) runSearch(lastQuery);
    });

    searchForm.addEventListener("submit", (event) => {
      if (selectedIndex >= 0 && items[selectedIndex]) {
        event.preventDefault();
        window.location.href = items[selectedIndex].href;
      }
    });

    searchInput.addEventListener("keydown", (event) => {
      const count = items.length;
      if (event.key === "ArrowDown" && count > 0) {
        event.preventDefault();
        selectedIndex = (selectedIndex + 1) % count;
        paintSelection();
      } else if (event.key === "ArrowUp" && count > 0) {
        event.preventDefault();
        selectedIndex = (selectedIndex - 1 + count) % count;
        paintSelection();
      } else if (event.key === "Escape") {
        hideDropdown();
        searchInput.blur();
      }
    });

    document.addEventListener("click", (event) => {
      if (!searchForm.contains(event.target)) hideDropdown();
    });

    document.addEventListener("keydown", (event) => {
      const tag = (event.target.tagName || "").toLowerCase();
      const typing = tag === "input" || tag === "textarea" || tag === "select" || event.target.isContentEditable;
      if (event.key === "/" && !typing) {
        event.preventDefault();
        searchInput.focus();
      }
    });
  }

  /* ---------------------------------------------------------
     Hero slider
     --------------------------------------------------------- */

  (function heroSlider() {
    const hero = $("[data-hero]");
    if (!hero) return;
    const slides = $$(".hero-slide", hero);
    if (slides.length < 2) return;

    const dots = $$(".hero-dot", hero);
    const current = $("#hero-current");
    const total = $("#hero-total");
    let index = 0;
    let timer = 0;

    const paint = (next) => {
      index = (next + slides.length) % slides.length;
      slides.forEach((slide, i) => {
        const active = i === index;
        slide.classList.toggle("is-active", active);
        slide.setAttribute("aria-hidden", active ? "false" : "true");
      });
      dots.forEach((dot, i) => dot.classList.toggle("is-active", i === index));
      if (current) current.textContent = String(index + 1).padStart(2, "0");
      if (total) total.textContent = String(slides.length);
    };

    const play = () => {
      clearInterval(timer);
      timer = window.setInterval(() => paint(index + 1), 7000);
    };

    const stop = () => clearInterval(timer);

    $("#hero-prev")?.addEventListener("click", () => {
      paint(index - 1);
      play();
    });
    $("#hero-next")?.addEventListener("click", () => {
      paint(index + 1);
      play();
    });
    dots.forEach((dot, i) =>
      dot.addEventListener("click", () => {
        paint(i);
        play();
      }),
    );

    hero.addEventListener("pointerenter", stop);
    hero.addEventListener("pointerleave", play);
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) stop();
      else play();
    });

    paint(0);
    play();
  })();

  function currentAccount() {
    const user = window.aniVerseAuth?.getUser?.() || window.aniVerseAuthUser || null;
    if (!user) return null;
    const metadata = user.user_metadata || {};
    const email = user.email || "";
    const name = String(metadata.full_name || metadata.name || email.split("@")[0] || "Pengguna").trim();
    return {
      id: user.id,
      name,
      email,
      avatarUrl: metadata.avatar_url || metadata.picture || null,
    };
  }

  function getLocalComments() {
    const comments = [];
    try {
      for (let index = 0; index < localStorage.length; index += 1) {
        const key = localStorage.key(index);
        if (!key?.startsWith("aniverse:comments:")) continue;
        const episodeSlug = key.slice("aniverse:comments:".length);
        readStore(key).forEach((item) => comments.push({ ...item, episodeSlug }));
      }
    } catch (_) {}
    if (Array.isArray(window.aniVerseUserComments)) {
      comments.push(...window.aniVerseUserComments);
    }
    return comments.sort((a, b) => Number(b.createdAt) - Number(a.createdAt));
  }

  function updateAccountStats() {
    const historyCount = $("#profile-history-count");
    const commentCount = $("#profile-comment-count");
    const bookmarkCount = $("#profile-bookmark-count");
    if (historyCount) historyCount.textContent = String(getHistory().length);
    if (commentCount) commentCount.textContent = String(getLocalComments().length);
    if (bookmarkCount) bookmarkCount.textContent = String(getBookmarks().length);
  }

  function renderAccountActivity(mode = "all") {
    const list = $("#profile-activity-list");
    if (!list) return;
    const entries = [];
    if (mode !== "comments") {
      getHistory().forEach((item) => entries.push({ kind: "history", time: item.watchedAt, item }));
    }
    if (mode !== "history") {
      getLocalComments().forEach((item) => entries.push({ kind: "comment", time: item.createdAt, item }));
    }
    entries.sort((a, b) => Number(b.time || 0) - Number(a.time || 0));
    list.innerHTML = entries
      .slice(0, 16)
      .map(({ kind, item }) =>
        kind === "history"
          ? `<a class="profile-activity-item" href="${escapeHtml(item.href)}"><span class="activity-icon"><svg class="icon"><use href="#i-clock"/></svg></span><span><strong>${escapeHtml(item.title)}</strong><small>Menonton episode ${escapeHtml(item.episode || "—")} · ${escapeHtml(relativeTime(item.watchedAt))}</small></span><svg class="icon activity-arrow"><use href="#i-chevron-right"/></svg></a>`
          : `<a class="profile-activity-item" href="/watch/${encodeURIComponent(item.episodeSlug)}"><span class="activity-icon"><svg class="icon"><use href="#i-message"/></svg></span><span><strong>Berkomentar · ${escapeHtml(item.episodeTitle || "Episode anime")}</strong><small>${escapeHtml(item.text)} · ${escapeHtml(relativeTime(item.createdAt))}</small></span><svg class="icon activity-arrow"><use href="#i-chevron-right"/></svg></a>`,
      )
      .join("");
    if (entries.length === 0) list.innerHTML = `<p class="profile-activity-empty">Aktivitas akan muncul di sini.</p>`;
  }

  function initAccountPage() {
    const profile = $("#account-profile");
    if (!profile) return;

    const editForm = $("#profile-settings-form");
    const nameInput = $("#profile-display-name");
    const avatarFile = $("#profile-avatar-file");
    const avatarEdit = $("#profile-avatar-edit");
    const photoName = $("#profile-photo-name");
    let selectedAvatar = null;
    let previewUrl = "";
    const cropModal = $("#profile-avatar-crop-modal");
    const cropCanvas = $("#profile-avatar-crop-canvas");
    const cropZoom = $("#profile-avatar-crop-zoom");
    const cropApply = $("#profile-avatar-crop-apply");
    let cropImage = null;
    let cropObjectUrl = "";
    let cropOffsetX = 0;
    let cropOffsetY = 0;
    let cropDrag = null;
    const cropSize = cropCanvas?.width || 320;

    const cropGeometry = (size = cropSize) => {
      if (!cropImage) return null;
      const zoom = Math.max(1, Number(cropZoom?.value) || 1);
      const scale = Math.max(size / cropImage.naturalWidth, size / cropImage.naturalHeight) * zoom;
      const width = cropImage.naturalWidth * scale;
      const height = cropImage.naturalHeight * scale;
      const offsetScale = size / cropSize;
      const offsetX = cropOffsetX * offsetScale;
      const offsetY = cropOffsetY * offsetScale;
      const maxX = Math.max(0, (width - size) / 2);
      const maxY = Math.max(0, (height - size) / 2);
      const clampedX = Math.max(-maxX, Math.min(maxX, offsetX));
      const clampedY = Math.max(-maxY, Math.min(maxY, offsetY));
      cropOffsetX = clampedX / offsetScale;
      cropOffsetY = clampedY / offsetScale;
      return { width, height, x: (size - width) / 2 + clampedX, y: (size - height) / 2 + clampedY };
    };

    const drawAvatarCrop = (canvas = cropCanvas) => {
      if (!canvas || !cropImage) return;
      const context = canvas.getContext("2d");
      const size = canvas.width;
      const geometry = cropGeometry(size);
      if (!geometry) return;
      context.clearRect(0, 0, size, size);
      context.save();
      if (canvas === cropCanvas) {
        context.beginPath();
        context.arc(size / 2, size / 2, size / 2, 0, Math.PI * 2);
        context.clip();
      }
      context.drawImage(cropImage, geometry.x, geometry.y, geometry.width, geometry.height);
      context.restore();
    };

    const closeAvatarCrop = () => {
      if (cropModal) cropModal.hidden = true;
      cropImage = null;
      cropDrag = null;
      if (cropObjectUrl) URL.revokeObjectURL(cropObjectUrl);
      cropObjectUrl = "";
      if (avatarFile) avatarFile.value = "";
    };

    const prepareAvatarCrop = async (file) => {
      closeAvatarCrop();
      cropObjectUrl = URL.createObjectURL(file);
      const image = new Image();
      image.decoding = "async";
      image.src = cropObjectUrl;
      try {
        await image.decode();
      } catch {
        closeAvatarCrop();
        toast("Foto tidak bisa dibuka. Coba pilih gambar lain.", "i-info");
        return;
      }
      if (image.naturalWidth > 20000 || image.naturalHeight > 20000 || image.naturalWidth * image.naturalHeight > 100_000_000) {
        closeAvatarCrop();
        toast("Resolusi foto terlalu besar untuk diproses di perangkat ini.", "i-info");
        return;
      }
      cropImage = image;
      cropOffsetX = 0;
      cropOffsetY = 0;
      if (cropZoom) cropZoom.value = "1";
      drawAvatarCrop();
      if (cropModal) cropModal.hidden = false;
    };

    const renderProfile = () => {
      const account = currentAccount();
      const nameNode = $("#profile-name");
      const emailNode = $("#profile-email");
      const avatarNode = $("#profile-avatar");
      const editToggle = $("#profile-edit-toggle");
      const badge = $("#profile-user-badge");
      if (nameNode) nameNode.textContent = account?.name || "Tamu";
      if (emailNode) {
        emailNode.textContent = account
          ? "Profil dan aktivitas publik AniVerse"
          : "Masuk dengan Google untuk mengatur profil dan berkomentar.";
      }
      if (avatarNode) {
        const imageUrl = previewUrl || account?.avatarUrl;
        avatarNode.innerHTML = imageUrl
          ? `<img src="${escapeHtml(imageUrl)}" alt="" referrerpolicy="no-referrer">`
          : escapeHtml(initials(account?.name || "Tamu"));
      }
      if (editToggle) editToggle.hidden = !account;
      if (badge) badge.hidden = !account;
      if (avatarEdit) avatarEdit.hidden = !account || !editForm?.hidden;
      if (nameInput && account && document.activeElement !== nameInput) nameInput.value = account.name;
    };

    const resetAvatarSelection = () => {
      closeAvatarCrop();
      selectedAvatar = null;
      if (avatarFile) avatarFile.value = "";
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      previewUrl = "";
      if (photoName) photoName.textContent = "Foto profil saat ini";
    };

    $("#profile-edit-toggle")?.addEventListener("click", () => {
      if (!editForm) return;
      editForm.hidden = !editForm.hidden;
      $("#profile-edit-toggle")?.setAttribute("aria-expanded", String(!editForm.hidden));
      if (editForm.hidden) resetAvatarSelection();
      else if (nameInput) nameInput.value = currentAccount()?.name || "";
      renderProfile();
    });

    const openAvatarPicker = () => avatarFile?.click();
    avatarEdit?.addEventListener("click", openAvatarPicker);
    $("#profile-photo-choose")?.addEventListener("click", openAvatarPicker);
    avatarFile?.addEventListener("change", () => {
      const file = avatarFile.files?.[0] || null;
      if (!file) return;
      const declaredType = String(file.type || "").split(";")[0].trim().toLowerCase();
      const normalizedType = ({ "image/jpg": "image/jpeg", "image/pjpeg": "image/jpeg" })[declaredType] || declaredType;
      const extension = file.name.split(".").pop()?.toLowerCase();
      const extensionTypes = { jpg: "image/jpeg", jpeg: "image/jpeg", png: "image/png", webp: "image/webp" };
      const supportedType = ["", "application/octet-stream"].includes(normalizedType)
        ? extensionTypes[extension]
        : normalizedType;
      if (!["image/jpeg", "image/png", "image/webp"].includes(supportedType)) {
        avatarFile.value = "";
        return toast("Pilih foto JPG, PNG, atau WebP", "i-info");
      }
      if (file.size > 20 * 1024 * 1024) {
        avatarFile.value = "";
        return toast("Ukuran foto maksimal 20 MB", "i-info");
      }
      prepareAvatarCrop(file);
    });

    cropZoom?.addEventListener("input", drawAvatarCrop);
    cropCanvas?.addEventListener("pointerdown", (event) => {
      if (!cropImage) return;
      cropDrag = { pointerId: event.pointerId, x: event.clientX, y: event.clientY };
      cropCanvas.setPointerCapture(event.pointerId);
    });
    cropCanvas?.addEventListener("pointermove", (event) => {
      if (!cropDrag || cropDrag.pointerId !== event.pointerId) return;
      const bounds = cropCanvas.getBoundingClientRect();
      cropOffsetX += (event.clientX - cropDrag.x) * (cropCanvas.width / bounds.width);
      cropOffsetY += (event.clientY - cropDrag.y) * (cropCanvas.height / bounds.height);
      cropDrag.x = event.clientX;
      cropDrag.y = event.clientY;
      drawAvatarCrop();
    });
    ["pointerup", "pointercancel", "lostpointercapture"].forEach((name) =>
      cropCanvas?.addEventListener(name, () => { cropDrag = null; }),
    );
    $$('[data-avatar-crop-cancel]', cropModal || document).forEach((button) =>
      button.addEventListener("click", closeAvatarCrop),
    );
    cropApply?.addEventListener("click", async () => {
      if (!cropImage || !cropCanvas) return;
      const output = document.createElement("canvas");
      output.width = 512;
      output.height = 512;
      drawAvatarCrop(output);
      const makeBlob = (type, quality) => new Promise((resolve) => output.toBlob(resolve, type, quality));
      let blob = await makeBlob("image/webp", 0.88);
      let extension = "webp";
      if (!blob) {
        blob = await makeBlob("image/png");
        extension = "png";
      }
      if (blob?.size > 2 * 1024 * 1024) blob = await makeBlob("image/webp", 0.7);
      if (!blob || blob.size > 2 * 1024 * 1024) {
        toast("Crop foto masih terlalu besar. Coba kecilkan kualitas gambar sumber.", "i-info");
        return;
      }
      if (blob.type === "image/png") extension = "png";
      else if (blob.type === "image/jpeg") extension = "jpg";
      const croppedFile = new File([blob], `avatar-${Date.now()}.${extension}`, { type: blob.type });
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      selectedAvatar = croppedFile;
      previewUrl = URL.createObjectURL(croppedFile);
      if (photoName) photoName.textContent = `Crop foto · ${croppedFile.name}`;
      closeAvatarCrop();
      renderProfile();
    });

    $("#profile-edit-cancel")?.addEventListener("click", () => {
      resetAvatarSelection();
      if (editForm) editForm.hidden = true;
      $("#profile-edit-toggle")?.setAttribute("aria-expanded", "false");
      renderProfile();
    });

    const loadAccountComments = async () => {
      window.aniVerseUserComments = [];
      const account = currentAccount();
      const client = window.aniVerseSupabase;
      if (account && client) {
        const { data, error } = await client
          .from("episode_comments")
          .select("episode_slug,episode_title,body,created_at")
          .eq("author_id", account.id)
          .order("created_at", { ascending: false })
          .limit(100);
        if (!error) {
          window.aniVerseUserComments = (data || []).map((row) => ({
            episodeSlug: row.episode_slug,
            episodeTitle: row.episode_title,
            text: row.body,
            createdAt: Date.parse(row.created_at) || Date.now(),
          }));
        }
      }
      updateAccountStats();
      renderAccountActivity();
    };

    renderProfile();
    window.aniVerseUserComments = [];
    window.addEventListener("aniverse:auth-change", () => {
      renderProfile();
      loadAccountComments();
    });

    updateAccountStats();
    renderAccountActivity();
    loadAccountComments();

    $$("[data-account-tab]").forEach((button) => {
      button.addEventListener("click", () => {
        $$("[data-account-tab]").forEach((tab) => tab.classList.toggle("is-active", tab === button));
        renderAccountActivity(button.dataset.accountTab);
      });
    });

    editForm?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const account = currentAccount();
      const api = window.aniVerseAuth;
      const status = $("#profile-settings-status");
      const submit = $("#profile-settings-save");
      const name = String(nameInput?.value || "").trim();
      if (!account || !api?.updateProfile) return toast("Masuk dengan Google untuk mengubah profil", "i-user");
      if (!name || name.length > 40) return toast("Nama profil harus 1–40 karakter", "i-info");
      if (submit) submit.disabled = true;
      if (status) status.textContent = "Menyimpan perubahan…";
      try {
        await api.updateProfile(name, selectedAvatar);
        resetAvatarSelection();
        editForm.hidden = true;
        $("#profile-edit-toggle")?.setAttribute("aria-expanded", "false");
        renderProfile();
        if (status) status.textContent = "Profil berhasil disimpan.";
        toast("Profil berhasil diperbarui");
      } catch (error) {
        if (status) status.textContent = "Perubahan gagal disimpan. Coba lagi.";
        toast(`Gagal menyimpan profil: ${error.message}`, "i-info");
      } finally {
        if (submit) submit.disabled = false;
      }
    });
  }

  /* ---------------------------------------------------------
     Unduhan & offline
     --------------------------------------------------------- */

  function videoDownloadName(title, quality = "") {
    const base = `${title || "AniVerse"} ${quality || ""}`
      .normalize("NFKD")
      .replace(/[^\w .-]/g, " ")
      .replace(/[. ]+$/g, "")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 110) || "AniVerse video";
    return `${base}.mp4`;
  }

  function deviceDownloadUrl(id) {
    return new URL(`/__device-downloads/${encodeURIComponent(id)}`, location.origin).href;
  }

  function openDownloadHandleDb() {
    return new Promise((resolve, reject) => {
      if (!window.indexedDB) return reject(new Error("Penyimpanan folder tidak didukung."));
      const request = indexedDB.open(DOWNLOAD_HANDLE_DB, 1);
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains("directories")) request.result.createObjectStore("directories");
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error || new Error("Penyimpanan folder gagal dibuka."));
    });
  }

  async function storeDownloadDirectory(id, directoryHandle) {
    const db = await openDownloadHandleDb();
    return new Promise((resolve, reject) => {
      const transaction = db.transaction("directories", "readwrite");
      transaction.objectStore("directories").put(directoryHandle, id);
      transaction.oncomplete = () => { db.close(); resolve(); };
      transaction.onerror = () => { db.close(); reject(transaction.error || new Error("Izin folder gagal disimpan.")); };
      transaction.onabort = () => { db.close(); reject(transaction.error || new Error("Penyimpanan folder dibatalkan.")); };
    });
  }

  async function getDownloadDirectory(id) {
    const db = await openDownloadHandleDb();
    return new Promise((resolve, reject) => {
      const transaction = db.transaction("directories", "readonly");
      const request = transaction.objectStore("directories").get(id);
      request.onsuccess = () => resolve(request.result || null);
      request.onerror = () => reject(request.error || new Error("Folder unduhan gagal dibuka."));
      transaction.oncomplete = () => db.close();
      transaction.onerror = () => { db.close(); reject(transaction.error || new Error("Folder unduhan gagal dibaca.")); };
    });
  }

  async function removeDownloadDirectory(id) {
    const db = await openDownloadHandleDb();
    return new Promise((resolve, reject) => {
      const transaction = db.transaction("directories", "readwrite");
      transaction.objectStore("directories").delete(id);
      transaction.oncomplete = () => { db.close(); resolve(); };
      transaction.onerror = () => { db.close(); reject(transaction.error || new Error("Izin folder gagal dihapus.")); };
      transaction.onabort = () => { db.close(); reject(transaction.error || new Error("Penghapusan izin folder dibatalkan.")); };
    });
  }

  async function saveVideoOnDevice(button) {
    const title = button.dataset.title || "Video AniVerse";
    const server = button.dataset.server || "AniVerse";
    const quality = button.dataset.quality || "";
    const sourceUrl = button.dataset.sourceUrl || "";
    const savedItems = readStore(STORE.downloads);
    const baseFilename = videoDownloadName(title, quality);
    const usedFilenames = new Set(savedItems.map((item) => String(item.filename || "").toLowerCase()));
    const extensionIndex = baseFilename.toLowerCase().lastIndexOf(".mp4");
    const filenameStem = extensionIndex >= 0 ? baseFilename.slice(0, extensionIndex) : baseFilename;
    let filename = baseFilename;
    let copyNumber = 2;
    while (usedFilenames.has(filename.toLowerCase())) {
      filename = `${filenameStem} (${copyNumber}).mp4`;
      copyNumber += 1;
    }
    if (!sourceUrl) throw new Error("Sumber video tidak ditemukan.");

    const apiUrl = new URL("/api/video", location.origin);
    apiUrl.searchParams.set("url", sourceUrl);
    apiUrl.searchParams.set("download", "true");
    apiUrl.searchParams.set("filename", filename);
    const id = globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const cacheUrl = deviceDownloadUrl(id);
    let directoryHandle = null;
    let fileHandle = null;
    let writable = null;
    let storedInBrowser = false;
    let storedInFolder = false;

    if (typeof window.showDirectoryPicker === "function") {
      try {
        directoryHandle = await window.showDirectoryPicker({ id: "aniverse-downloads", mode: "readwrite" });
      } catch (error) {
        if (error?.name === "AbortError") throw new Error("Pemilihan folder dibatalkan.");
      }
    }

    if (directoryHandle) {
      try {
        await storeDownloadDirectory(id, directoryHandle);
      } catch (_) {
        await removeDownloadDirectory(id).catch(() => {});
        directoryHandle = null;
      }
    }

    if (directoryHandle) {
      try {
        const response = await fetch(apiUrl.href);
        if (!response.ok || response.status !== 200) throw new Error(response.status === 400 ? "Sumber video tidak valid." : "Video gagal diunduh lengkap dari sumber.");
        const contentType = response.headers.get("content-type") || "";
        if (contentType.includes("text/html")) throw new Error("Sumber tidak mengirimkan file video.");
        fileHandle = await directoryHandle.getFileHandle(filename, { create: true });
        writable = await fileHandle.createWritable();
        if (response.body) await response.body.pipeTo(writable);
        else {
          await writable.write(await response.blob());
          await writable.close();
        }
        storedInFolder = true;
      } catch (error) {
        await removeDownloadDirectory(id).catch(() => {});
        throw error;
      }
    } else if ("caches" in window) {
      try {
        if (navigator.storage?.persist) await navigator.storage.persist().catch(() => false);
        const response = await fetch(apiUrl.href);
        if (!response.ok || response.status !== 200) throw new Error(response.status === 400 ? "Sumber video tidak valid." : "Video gagal diunduh lengkap dari sumber.");
        const contentType = response.headers.get("content-type") || "";
        if (contentType.includes("text/html")) throw new Error("Sumber tidak mengirimkan file video.");
        const cache = await caches.open(VIDEO_DOWNLOAD_CACHE);
        await cache.put(cacheUrl, response);
        storedInBrowser = true;
      } catch (error) {
        await caches.open(VIDEO_DOWNLOAD_CACHE).then((cache) => cache.delete(cacheUrl)).catch(() => {});
        if (error?.name === "QuotaExceededError") throw new Error("Ruang penyimpanan perangkat tidak cukup. Hapus unduhan lain lalu coba lagi.");
        if (error instanceof TypeError && !navigator.onLine) throw new Error("Sambungkan internet untuk mengunduh video.");
        throw new Error(error?.message || "Penyimpanan perangkat penuh atau tidak tersedia.");
      }
    } else {
      const link = document.createElement("a");
      link.href = apiUrl.href;
      link.download = filename;
      link.rel = "nofollow";
      document.body.append(link);
      link.click();
      link.remove();
    }

    const downloads = readStore(STORE.downloads);
    downloads.unshift({
      id,
      title,
      server,
      quality,
      filename,
      sourceUrl,
      cacheUrl: storedInBrowser ? cacheUrl : "",
      downloadUrl: storedInBrowser ? cacheUrl : storedInFolder ? "" : apiUrl.href,
      storage: storedInFolder ? "filesystem" : storedInBrowser ? "browser" : "native",
      savedAt: Date.now(),
    });
    if (!writeStore(STORE.downloads, downloads)) {
      if (storedInBrowser) await caches.open(VIDEO_DOWNLOAD_CACHE).then((cache) => cache.delete(cacheUrl)).catch(() => {});
      if (storedInFolder) {
        await directoryHandle.removeEntry(filename).catch(() => {});
        await removeDownloadDirectory(id).catch(() => {});
      }
      throw new Error("Penyimpanan daftar unduhan penuh.");
    }
    updateAccountStats();
    return storedInFolder ? "folder" : storedInBrowser ? "browser" : "native";
  }

  function initDownloadCollection() {
    const list = $("#saved-download-list");
    if (!list) return;
    const empty = $("#downloads-empty");
    const render = () => {
      const items = readStore(STORE.downloads);
      if (empty) empty.hidden = items.length > 0;
      list.innerHTML = items
        .map(
          (item, index) => `
        <article class="saved-download-item">
          <span class="download-file-icon"><svg class="icon"><use href="#i-download"/></svg></span>
          <div class="saved-download-body">
            <strong>${escapeHtml(item.title)}</strong>
            <span>${escapeHtml(item.server)}${item.quality ? ` · ${escapeHtml(item.quality)}` : ""}</span>
            <small>${item.storage === "filesystem" ? "File tersimpan di folder pilihan" : item.cacheUrl ? "Tersimpan offline di browser" : item.storage === "native" ? "Diunduh lewat browser · kelola file lewat aplikasi File" : "Tautan lama · belum tersimpan sebagai video"} · ${escapeHtml(relativeTime(item.savedAt))}</small>
          </div>
          ${item.downloadUrl || item.cacheUrl ? `<a class="btn btn-gold btn-sm" href="${escapeHtml(item.downloadUrl || item.cacheUrl)}" download="${escapeHtml(item.filename || "aniverse-video.mp4")}">Unduh file</a>` : item.url ? `<a class="btn btn-ghost btn-sm" href="${escapeHtml(item.url)}" target="_blank" rel="noopener noreferrer nofollow">Tautan lama</a>` : ""}
          <button class="icon-btn remove-download" type="button" data-remove-download="${index}" aria-label="Hapus video tersimpan"><svg class="icon"><use href="#i-x"/></svg></button>
        </article>`,
        )
        .join("");
    };
    render();
    list.addEventListener("click", async (event) => {
      const button = event.target.closest("[data-remove-download]");
      if (!button) return;
      const items = readStore(STORE.downloads);
      const [removed] = items.splice(Number(button.dataset.removeDownload), 1);
      if (removed?.cacheUrl && "caches" in window) {
        try {
          await caches.open(VIDEO_DOWNLOAD_CACHE).then((cache) => cache.delete(removed.cacheUrl));
        } catch (_) {
          return toast("Video belum berhasil dihapus. Coba lagi.", "i-info");
        }
      }
      if (removed?.storage === "filesystem" && removed.id) {
        try {
          const directory = await getDownloadDirectory(removed.id);
          if (!directory) throw new Error("Izin folder tidak tersedia. Hapus file lewat aplikasi File.");
          if (directory.queryPermission) {
            let permission = await directory.queryPermission({ mode: "readwrite" });
            if (permission !== "granted" && directory.requestPermission) {
              permission = await directory.requestPermission({ mode: "readwrite" });
            }
            if (permission !== "granted") throw new Error("Izin folder ditolak. Hapus file lewat aplikasi File.");
          }
          await directory.removeEntry(removed.filename).catch((error) => {
            if (error?.name !== "NotFoundError") throw error;
          });
          await removeDownloadDirectory(removed.id);
        } catch (error) {
          return toast(error.message || "File belum berhasil dihapus.", "i-info");
        }
      }
      writeStore(STORE.downloads, items);
      render();
      updateAccountStats();
      toast(removed?.storage === "filesystem" ? "File video dihapus dari folder" : removed?.cacheUrl ? "Video dihapus dari penyimpanan perangkat" : "Item unduhan dihapus", "i-trash");
    });
  }

  function initEpisodeDownloadList() {
    const list = $("#episode-download-list");
    const toggle = $("#download-more-button");
    if (!list || !toggle) return;
    const rows = $$('[data-download-item]', list);
    const visibleCount = 3;
    if (rows.length <= visibleCount) return;
    let expanded = false;
    const paint = () => {
      rows.forEach((row, index) => {
        row.hidden = !expanded && index >= visibleCount;
      });
      toggle.hidden = false;
      toggle.setAttribute("aria-expanded", expanded ? "true" : "false");
      toggle.textContent = expanded ? "Ringkas tautan" : `Tampilkan semua tautan (${rows.length})`;
    };
    toggle.addEventListener("click", () => {
      expanded = !expanded;
      paint();
    });
    paint();
  }

  function initOfflineCollection() {
    const grid = $("#offline-grid");
    if (!grid) return;
    const empty = $("#offline-empty");
    const render = () => {
      const items = readStore(STORE.offline);
      if (empty) empty.hidden = items.length > 0;
      grid.innerHTML = items
        .map((item) => `
        <article class="offline-card">
          <a class="poster-card" href="${escapeHtml(item.href)}">
            <div class="poster">
              ${item.image ? `<img src="${escapeHtml(item.image)}" alt="${escapeHtml(item.title)}" loading="lazy">` : `<span class="poster-fallback">${escapeHtml(initials(item.title))}</span>`}
              <span class="poster-shade" aria-hidden="true"></span>
              <div class="poster-top"><span class="chip-format">Offline</span></div>
            </div>
            <div class="poster-info">
              <h3 class="poster-title">${escapeHtml(item.title)}</h3>
              <p class="poster-sub">Disimpan ${escapeHtml(relativeTime(item.savedAt))}</p>
            </div>
          </a>
          <button type="button" class="text-btn offline-remove" data-remove-offline="${escapeHtml(item.href)}">Hapus simpanan</button>
        </article>`,
        )
        .join("");
    };
    render();
    grid.addEventListener("click", async (event) => {
      const button = event.target.closest("[data-remove-offline]");
      if (!button) return;
      const href = button.dataset.removeOffline;
      try {
        const cache = await caches.open(OFFLINE_PAGE_CACHE);
        await cache.delete(new URL(href, location.origin).href);
      } catch (_) {}
      writeStore(STORE.offline, readStore(STORE.offline).filter((item) => item.href !== href));
      render();
      updateAccountStats();
      toast("Simpanan offline dihapus", "i-trash");
    });
  }

  /* ---------------------------------------------------------
     Komentar
     --------------------------------------------------------- */

  function initComments() {
    const panel = $("#comments-panel");
    if (!panel) return;
    const slug = panel.dataset.commentSlug;
    const form = $("#comment-form");
    const gate = $("#comment-gate");
    const list = $("#comment-list");
    const status = $("#comments-cloud-status");
    let rows = [];
    let likedIds = new Set();
    let loading = false;

    const profileAvatar = (name, url) =>
      url
        ? `<span class="comment-avatar"><img src="${escapeHtml(url)}" alt="" loading="lazy" referrerpolicy="no-referrer"></span>`
        : `<span class="comment-avatar">${escapeHtml(initials(name))}</span>`;

    const nestComments = (records) => {
      const nodes = records.map((row) => {
        const profile = Array.isArray(row.profile) ? row.profile[0] : row.profile;
        return {
          id: row.id,
          parentId: row.parent_id,
          author: profile?.display_name || "Pengguna",
          avatarUrl: profile?.avatar_url || "",
          text: row.body,
          episodeTitle: row.episode_title,
          createdAt: Date.parse(row.created_at) || Date.now(),
          likes: Number(row.likes_count) || 0,
          replies: [],
        };
      });
      const byId = new Map(nodes.map((item) => [item.id, item]));
      const roots = [];
      nodes.forEach((item) => {
        const parent = item.parentId ? byId.get(item.parentId) : null;
        if (parent) parent.replies.push(item);
        else roots.push(item);
      });
      roots.sort((a, b) => b.createdAt - a.createdAt);
      roots.forEach((item) => item.replies.sort((a, b) => a.createdAt - b.createdAt));
      return roots;
    };

    const renderReply = (item) => `
      <div class="comment-reply" id="comment-${escapeHtml(item.id)}">
        ${profileAvatar(item.author, item.avatarUrl)}
        <div><strong>${escapeHtml(item.author)}</strong><small>${escapeHtml(relativeTime(item.createdAt))}</small><p>${escapeHtml(item.text)}</p></div>
      </div>`;

    const renderComment = (item) => `
      <article class="comment-item" id="comment-${escapeHtml(item.id)}">
        ${profileAvatar(item.author, item.avatarUrl)}
        <div class="comment-body">
          <div class="comment-meta"><strong>${escapeHtml(item.author)}</strong><span>${escapeHtml(relativeTime(item.createdAt))}</span></div>
          <p>${escapeHtml(item.text)}</p>
          <div class="comment-actions">
            <button type="button" data-comment-like="${escapeHtml(item.id)}" class="${likedIds.has(item.id) ? "is-liked" : ""}" aria-pressed="${likedIds.has(item.id)}">♥ ${item.likes}</button>
            <button type="button" data-comment-reply-toggle="${escapeHtml(item.id)}">Balas${item.replies.length ? ` · ${item.replies.length}` : ""}</button>
          </div>
          <form class="reply-form" data-reply-form="${escapeHtml(item.id)}" autocomplete="off" hidden>
            <input name="reply" type="text" autocomplete="off" maxlength="300" placeholder="Tulis balasan..." required>
            <button class="btn btn-ghost btn-sm" type="submit">Balas</button>
          </form>
          ${item.replies.map(renderReply).join("")}
        </div>
      </article>`;

    function render() {
      const account = currentAccount();
      if (gate) gate.hidden = Boolean(account);
      if (form) form.hidden = !account;
      const items = nestComments(rows);
      const count = $("#comment-count");
      const total = rows.length;
      if (count) count.textContent = `${total} komentar`;
      if (list) list.innerHTML = items.map(renderComment).join("");
      const empty = $("#comments-empty");
      if (empty) empty.hidden = total > 0;
    }

    const loadComments = async () => {
      const client = window.aniVerseSupabase;
      if (!client || loading) {
        if (!client && status) status.textContent = "Komentar online belum dikonfigurasi.";
        return;
      }
      loading = true;
      if (status) status.textContent = "Memuat komentar…";
      try {
        const { data, error } = await client
          .from("episode_comments")
          .select("id,episode_slug,episode_title,author_id,body,parent_id,likes_count,created_at,profile:profiles!episode_comments_author_id_fkey(display_name,avatar_url)")
          .eq("episode_slug", slug)
          .order("created_at", { ascending: true })
          .limit(1000);
        if (error) throw error;
        rows = data || [];
        likedIds = new Set();
        const account = currentAccount();
        if (account) {
          const { data: likes, error: likesError } = await client
            .from("episode_comment_likes")
            .select("comment_id")
            .eq("user_id", account.id);
          if (likesError) throw likesError;
          likedIds = new Set((likes || []).map((item) => item.comment_id));
        }
        if (status) status.textContent = "";
        render();
      } catch (error) {
        if (status) status.textContent = "Komentar belum dapat dimuat. Pastikan supabase/schema.sql sudah dijalankan.";
        console.error("AniVerse comments:", error);
        render();
      } finally {
        loading = false;
      }
    };

    const insertComment = async (text, parentId = null) => {
      const account = currentAccount();
      const client = window.aniVerseSupabase;
      if (!account || !client) return toast("Masuk dengan Google untuk berkomentar", "i-user");
      const { error } = await client.from("episode_comments").insert({
        episode_slug: slug,
        episode_title: panel.dataset.commentTitle,
        author_id: account.id,
        body: text,
        parent_id: parentId,
      });
      if (error) {
        toast("Komentar gagal disimpan. Coba lagi sebentar.", "i-info");
        if (status) status.textContent = "Gagal menyimpan komentar ke database.";
        return false;
      }
      await loadComments();
      updateAccountStats();
      return true;
    };

    form?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const input = $("#comment-input");
      const text = input.value.trim();
      if (!text) return;
      const submit = $("button[type='submit']", form);
      if (submit) submit.disabled = true;
      if (await insertComment(text)) input.value = "";
      if (submit) submit.disabled = false;
    });

    panel.addEventListener("click", async (event) => {
      const replyToggle = event.target.closest("[data-comment-reply-toggle]");
      if (replyToggle) {
        if (!currentAccount()) return toast("Masuk dengan Google untuk membalas", "i-user");
        const replyForm = $(`[data-reply-form="${CSS.escape(replyToggle.dataset.commentReplyToggle)}"]`, panel);
        if (replyForm) {
          replyForm.hidden = !replyForm.hidden;
          if (!replyForm.hidden) $("input", replyForm).focus();
        }
        return;
      }
      const likeButton = event.target.closest("[data-comment-like]");
      if (likeButton) {
        const account = currentAccount();
        const client = window.aniVerseSupabase;
        if (!account || !client) return toast("Masuk dengan Google untuk menyukai komentar", "i-user");
        likeButton.disabled = true;
        const commentId = likeButton.dataset.commentLike;
        let error;
        if (likedIds.has(commentId)) {
          ({ error } = await client.from("episode_comment_likes").delete().eq("comment_id", commentId).eq("user_id", account.id));
        } else {
          ({ error } = await client.from("episode_comment_likes").insert({ comment_id: commentId, user_id: account.id }));
        }
        if (error) toast("Suka gagal disimpan. Coba lagi.", "i-info");
        await loadComments();
      }
    });

    panel.addEventListener("submit", async (event) => {
      const replyForm = event.target.closest("[data-reply-form]");
      if (!replyForm) return;
      event.preventDefault();
      const input = $("input[name='reply']", replyForm);
      const text = input.value.trim();
      if (!text) return;
      const submit = $("button[type='submit']", replyForm);
      if (submit) submit.disabled = true;
      if (await insertComment(text, replyForm.dataset.replyForm)) input.value = "";
      if (submit) submit.disabled = false;
    });

    render();
    window.addEventListener("aniverse:auth-change", () => {
      render();
      loadComments();
    });
    loadComments();
  }

  /* ---------------------------------------------------------
     Simpan offline & unduhan
     --------------------------------------------------------- */

  async function saveOfflinePage(data) {
    if (!("serviceWorker" in navigator) || !("caches" in window)) {
      toast("Penyimpanan offline tidak didukung di perangkat ini", "i-info");
      return;
    }
    try {
      await navigator.serviceWorker.register("/service-worker.js", { scope: "/" });
      await navigator.serviceWorker.ready;
      const href = new URL(data.href, location.origin).href;
      const response = await fetch(href, { cache: "reload" });
      if (!response.ok) throw new Error("Halaman belum bisa disimpan");
      const cache = await caches.open(OFFLINE_PAGE_CACHE);
      await cache.put(href, response.clone());
      const items = readStore(STORE.offline).filter((item) => item.href !== data.href);
      items.unshift({ ...data, savedAt: Date.now() });
      if (!writeStore(STORE.offline, items.slice(0, 80))) throw new Error("Penyimpanan perangkat penuh");
      updateAccountStats();
      toast("Detail anime disimpan offline", "i-device");
    } catch (error) {
      toast(error.message || "Anime belum bisa disimpan offline", "i-info");
    }
  }

  document.addEventListener("click", (event) => {
    const offlineButton = event.target.closest("[data-save-offline]");
    if (offlineButton) {
      event.preventDefault();
      offlineButton.disabled = true;
      saveOfflinePage({
        slug: offlineButton.dataset.slug,
        title: offlineButton.dataset.title,
        image: offlineButton.dataset.image,
        href: offlineButton.dataset.href,
      }).finally(() => {
        offlineButton.disabled = false;
      });
      return;
    }

    const downloadButton = event.target.closest("[data-device-download]");
    if (downloadButton) {
      event.preventDefault();
      if (downloadButton.disabled) return;
      downloadButton.disabled = true;
      const status = downloadButton.closest("[data-download-item]")?.querySelector("[data-device-download-status]");
      if (status) status.textContent = "Mengunduh dan menyimpan video…";
      saveVideoOnDevice(downloadButton)
        .then((storageMode) => {
          if (status) {
            status.textContent = storageMode === "folder"
              ? "File disimpan ke folder yang dipilih."
              : storageMode === "browser"
                ? "Tersimpan offline di perangkat."
                : "Unduhan dimulai di browser.";
          }
          toast(storageMode === "folder" ? "Video disimpan ke folder pilihan" : storageMode === "browser" ? "Video tersimpan offline di perangkat" : "Unduhan video dimulai", "i-download");
        })
        .catch((error) => {
          if (status) status.textContent = error.message || "Video gagal disimpan.";
          toast(error.message || "Video gagal disimpan", "i-info");
        })
        .finally(() => { downloadButton.disabled = false; });
    }
  });

  function updateConnectivity() {
    const banner = $("#offline-banner");
    if (banner) banner.hidden = navigator.onLine;
    document.documentElement.classList.toggle("is-offline", !navigator.onLine);
  }
  window.addEventListener("online", updateConnectivity);
  window.addEventListener("offline", updateConnectivity);
  updateConnectivity();

  if ("serviceWorker" in navigator && location.protocol.startsWith("http")) {
    navigator.serviceWorker.register("/service-worker.js", { scope: "/" }).catch(() => {});
  }

  /* ---------------------------------------------------------
     Halaman detail: sinopsis & episode
     --------------------------------------------------------- */

  (function synopsisToggle() {
    const text = $("#synopsis-text");
    const button = $("#synopsis-toggle");
    if (!text || !button) return;
    if (text.scrollHeight <= text.clientHeight + 4) return;
    button.hidden = false;
    button.addEventListener("click", () => {
      const expanded = text.classList.toggle("is-expanded");
      button.textContent = expanded ? "Sembunyikan" : "Selengkapnya";
    });
  })();

  (function episodeTools() {
    const list = $("#episode-list");
    if (!list) return;
    const rows = $$(".episode-chip", list);
    const input = $("#episode-filter");
    const empty = $("#episode-empty");
    const sortButton = $("#episode-sort");
    let order = sortButton?.dataset.order || "desc";

    const applyFilter = () => {
      const query = (input?.value || "").trim().toLowerCase();
      let visible = 0;
      rows.forEach((row) => {
        const match = !query || (row.dataset.search || row.textContent).toLowerCase().includes(query);
        row.hidden = !match;
        if (match) visible += 1;
      });
      if (empty) empty.hidden = visible > 0;
    };

    const applyOrder = () => {
      const sorted = [...rows];
      if (order === "asc") sorted.reverse();
      sorted.forEach((row) => list.appendChild(row));
    };

    if (input) input.addEventListener("input", applyFilter);
    if (sortButton) {
      sortButton.addEventListener("click", () => {
        order = order === "desc" ? "asc" : "desc";
        sortButton.dataset.order = order;
        const label = $("span", sortButton);
        if (label) label.textContent = order === "desc" ? "Terbaru dulu" : "Terlama dulu";
        applyOrder();
      });
    }
    applyOrder();
  })();

  /* ---------------------------------------------------------
     Pemutar video: kualitas otomatis + sumber mati
     --------------------------------------------------------- */

  (function player() {
    const video = $("#video-player");
    const dataNode = $("#stream-data");
    const historyNode = $("#watch-history");
    if (!video || !dataNode) return;

    let streams = [];
    try {
      const parsed = JSON.parse(dataNode.textContent.trim() || "[]");
      streams = Array.isArray(parsed) ? parsed : [];
    } catch (_) {
      streams = [];
    }

    const direct = streams.filter((item) => item && item.direct && item.available && item.url);
    const groups = [];
    direct.forEach((item) => {
      let group = groups.find((entry) => entry.quality === item.quality);
      if (!group) {
        group = { quality: item.quality, rank: Number(item.rank) || 0, servers: [] };
        groups.push(group);
      }
      if (!group.servers.some((entry) => entry.url === item.url)) group.servers.push(item);
    });
    groups.sort((a, b) => b.rank - a.rank);

    const qualityControl = $("#player-quality-control");
    const qualityToggle = $("#player-settings-toggle");
    const qualityMenu = $("#player-quality-menu");
    const qualityButtons = $$("#player-quality-menu [data-quality]");
    const playbackRateButtons = $$("#player-speed-options [data-playback-rate]");
    const volumeControlPanel = $("#player-volume-control");
    const volumePopover = $("#player-volume-popover");
    const volumeControl = $("#player-volume");
    const volumeLabel = $("#player-volume-value");
    const audioBoostControl = $("#player-audio-boost");
    const audioBoostLabel = $("#player-audio-boost-value");
    const reportButton = $("#player-report");
    const status = $("#player-status");
    const bigPlay = $("#player-play");

    const stage = $("#watch-stage");
    const nextUrl = stage?.dataset.nextUrl || "";
    const autoReplayControl = $("#player-auto-replay");
    const autoContinueControl = $("#player-auto-continue");
    const endScreen = $("#player-end-screen");
    const endReplayButton = $("#player-end-replay");
    const scrub = $("#player-scrub");
    const scrubBuffer = $("#player-scrub-buffer");
    const scrubPlayed = $("#player-scrub-played");
    const scrubKnob = $("#player-scrub-knob");
    const toggleButton = $("#player-toggle");
    const muteButton = $("#player-mute");
    const timeLabel = $("#player-time");
    const fullscreenButton = $("#player-fullscreen");
    const gestureLayer = $("#player-gesture-layer");
    const seekFeedback = $("#player-seek-feedback");
    const speedFeedback = $("#player-speed-feedback");

    const slug = historyNode?.dataset.slug || "";
    const progressKey = `aniverse:progress:${slug}`;

    let mode = readValue(STORE.quality, "auto");
    if (mode !== "auto" && !groups.some((group) => group.quality === mode)) mode = "auto";
    let group = null;
    let serverIndex = 0;
    let started = false;
    let switching = false;
    let dead = new Set();
    let pausedBySwitch = false;
    let stallCount = 0;
    let lastStallAt = 0;
    let lastQualityChange = Date.now();
    let healthySince = 0;
    let upgradeArmed = false;
    let lastSaveAt = 0;
    let resumeApplied = false;
    let statusTimer = 0;
    let currentVolume = Math.max(0, Math.min(1, Number(readValue(STORE.volume, 0.8)) || 0));
    let previousVolume = currentVolume || 0.8;
    let userMuted = currentVolume === 0;
    let audioBoost = Math.max(1, Math.min(2, Number(readValue(STORE.audioBoost, 1)) || 1));
    let configuredPlaybackRate = Math.max(0.5, Math.min(2, Number(readValue(STORE.playbackRate, 1)) || 1));
    let speedLocked = false;
    let autoReplay = readValue(STORE.autoReplay, false) === true;
    let autoContinue = Boolean(nextUrl) && readValue(STORE.autoContinue, false) === true;
    if (!nextUrl || autoReplay) {
      autoContinue = false;
      writeValue(STORE.autoContinue, false);
    }
    let audioContext = null;
    let audioGain = null;
    let audioGraphAttempted = false;

    const setStatus = (text, error = false, quiet = false) => {
      if (!status) return;
      status.textContent = text;
      status.classList.toggle("is-error", Boolean(error));
      status.classList.toggle("is-quiet", Boolean(quiet));
      window.clearTimeout(statusTimer);
      // Pesan biasa (mis. "Siap diputar · 720p") hanya perlu sebentar dan
      // tidak boleh menutupi video saat menonton. Pesan error tetap terlihat.
      if (!error && !quiet) {
        statusTimer = window.setTimeout(() => {
          status.classList.add("is-quiet");
        }, 3500);
      }
    };

    /* --- Kontrol pemutar kustom --------------------------------------- */

    let controlsTimer = 0;
    let scrubbing = false;
    let scrubWasPlaying = false;
    let pseudoFullscreen = false;
    let seekFeedbackTimer = 0;
    let pendingTap = null;
    let activeGesture = null;

    const setIcon = (button, iconId) => {
      const use = button?.querySelector("use");
      if (use) use.setAttribute("href", `#${iconId}`);
    };

    const clampNumber = (value, min, max, fallback) => {
      const number = Number(value);
      return Number.isFinite(number) ? Math.max(min, Math.min(max, number)) : fallback;
    };

    const applyAudioSettings = () => {
      if (audioGain) {
        if (audioContext?.state === "suspended") audioContext.resume().catch(() => {});
        video.volume = 1;
        audioGain.gain.value = userMuted ? 0 : currentVolume * audioBoost;
      } else {
        video.volume = userMuted ? 0 : currentVolume;
      }
    };

    const ensureAudioGraph = () => {
      if (audioGain) {
        if (audioContext?.state === "suspended") audioContext.resume().catch(() => {});
        return true;
      }
      if (audioGraphAttempted) return false;
      audioGraphAttempted = true;
      const AudioContextClass = window.AudioContext || window.webkitAudioContext;
      if (!AudioContextClass) return false;

      let source = null;
      try {
        audioContext = new AudioContextClass();
        source = audioContext.createMediaElementSource(video);
        audioGain = audioContext.createGain();
        source.connect(audioGain);
        audioGain.connect(audioContext.destination);
        applyAudioSettings();
        if (audioContext.state === "suspended") audioContext.resume().catch(() => {});
        return true;
      } catch (_) {
        // Bila perutean Web Audio gagal, kembalikan audio native agar video
        // tetap terdengar dengan volume biasa.
        try {
          source?.connect(audioContext.destination);
        } catch (_) {}
        audioGain = null;
        video.volume = userMuted ? 0 : currentVolume;
        return false;
      }
    };

    const paintVolume = () => {
      if (volumeControl) {
        volumeControl.value = String(Math.round(currentVolume * 100));
        volumeControl.setAttribute("aria-label", `Volume ${Math.round(currentVolume * 100)} persen`);
      }
      if (volumeLabel) volumeLabel.textContent = `${Math.round(currentVolume * 100)}%`;
      paintMute();
    };

    const paintAudioBoost = () => {
      if (audioBoostControl) audioBoostControl.value = String(Math.round(audioBoost * 100));
      if (audioBoostLabel) audioBoostLabel.textContent = `${Math.round(audioBoost * 100)}%`;
    };

    const paintPlaybackRate = () => {
      playbackRateButtons.forEach((button) => {
        const selected = Number(button.dataset.playbackRate) === configuredPlaybackRate;
        button.classList.toggle("is-selected", selected);
        button.setAttribute("aria-pressed", selected ? "true" : "false");
      });
    };

    const paintAutoSettings = () => {
      if (autoReplayControl) autoReplayControl.checked = autoReplay;
      if (autoContinueControl) autoContinueControl.checked = autoContinue;
    };

    const closeEndScreen = () => {
      if (endScreen) endScreen.hidden = true;
      stage?.classList.remove("is-ended");
    };

    const replayEpisode = () => {
      closeEndScreen();
      started = true;
      ensureAudioGraph();
      try {
        video.currentTime = 0;
      } catch (_) {}
      video.play().catch(() => {});
      showControls(false);
    };

    const setPlaybackRate = (rate) => {
      const nextRate = clampNumber(rate, 0.5, 2, 1);
      try {
        video.playbackRate = nextRate;
      } catch (_) {}
    };

    const setConfiguredPlaybackRate = (rate) => {
      configuredPlaybackRate = clampNumber(rate, 0.5, 2, 1);
      try {
        video.defaultPlaybackRate = configuredPlaybackRate;
      } catch (_) {}
      setPlaybackRate(configuredPlaybackRate);
    };

    const toggleMute = () => {
      if (userMuted || currentVolume === 0) {
        currentVolume = previousVolume || 0.8;
        userMuted = false;
      } else {
        previousVolume = currentVolume;
        userMuted = true;
      }
      writeValue(STORE.volume, currentVolume);
      applyAudioSettings();
      paintVolume();
      showControls();
    };

    const showControls = (autoHide = true) => {
      if (!stage) return;
      stage.classList.add("is-controls");
      stage.classList.remove("is-idle");
      window.clearTimeout(controlsTimer);
      if (!autoHide) return;
      if (video.paused || scrubbing || (qualityMenu && !qualityMenu.hidden) || (volumePopover && !volumePopover.hidden)) return;
      controlsTimer = window.setTimeout(() => {
        if (video.paused || scrubbing || (qualityMenu && !qualityMenu.hidden) || (volumePopover && !volumePopover.hidden)) return;
        stage.classList.add("is-idle");
        stage.classList.remove("is-controls");
      }, 2600);
    };

    const hideControlsSoon = () => {
      window.clearTimeout(controlsTimer);
      controlsTimer = window.setTimeout(() => {
        if (!video.paused && !scrubbing && !(qualityMenu && !qualityMenu.hidden) && !(volumePopover && !volumePopover.hidden)) {
          stage?.classList.add("is-idle");
          stage?.classList.remove("is-controls");
        }
      }, 1600);
    };

    const paintPlayed = (ratio) => {
      const percent = `${Math.max(0, Math.min(1, ratio)) * 100}%`;
      if (scrubPlayed) scrubPlayed.style.width = percent;
      if (scrubKnob) scrubKnob.style.left = percent;
    };

    const paintBuffer = () => {
      if (!scrubBuffer) return;
      const duration = Number.isFinite(video.duration) ? video.duration : 0;
      if (duration <= 0) {
        scrubBuffer.style.width = "0%";
        return;
      }
      let bufferedEnd = 0;
      try {
        if (video.buffered.length) bufferedEnd = video.buffered.end(video.buffered.length - 1);
      } catch (_) {
        bufferedEnd = 0;
      }
      scrubBuffer.style.width = `${Math.min(100, (bufferedEnd / duration) * 100)}%`;
    };

    const paintTime = () => {
      const duration = Number.isFinite(video.duration) ? video.duration : 0;
      const current = Math.max(0, Math.min(duration || Number.MAX_SAFE_INTEGER, video.currentTime || 0));
      if (timeLabel) {
        timeLabel.textContent = `${formatDuration(current)} / ${formatDuration(duration)}`;
      }
      const ratio = duration > 0 ? current / duration : 0;
      if (!scrubbing) paintPlayed(ratio);
      scrub?.setAttribute("aria-valuenow", String(Math.round(ratio * 100)));
      paintBuffer();
    };

    const paintPlay = () => {
      const paused = video.paused || video.ended;
      setIcon(toggleButton, paused ? "i-play" : "i-pause");
      toggleButton?.setAttribute("aria-label", paused ? "Putar" : "Jeda");
      if (started && bigPlay) bigPlay.classList.toggle("is-hidden", !paused);
    };

    const paintMute = () => {
      const muted = userMuted || currentVolume === 0;
      setIcon(muteButton, muted ? "i-volume-mute" : "i-volume");
      muteButton?.setAttribute("aria-label", muted ? "Nyalakan suara" : "Bisukan");
    };

    const togglePlay = () => {
      ensureAudioGraph();
      if (!started) {
        startPlayback();
        return;
      }
      if (video.paused || video.ended) {
        if (video.ended) {
          try {
            video.currentTime = 0;
          } catch (_) {}
        }
        video.play().catch(() => {});
      } else {
        video.pause();
      }
      showControls();
    };

    const seekTo = (time) => {
      if (!Number.isFinite(video.duration) || video.duration <= 0) return;
      const next = Math.max(0, Math.min(video.duration, time));
      try {
        video.currentTime = next;
      } catch (_) {}
      paintTime();
      showControls();
    };

    const ratioFromPointer = (event) => {
      if (!scrub) return 0;
      const rect = scrub.getBoundingClientRect();
      if (rect.width <= 0) return 0;
      return Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
    };

    const showSeekFeedback = (direction) => {
      if (!seekFeedback) return;
      seekFeedback.textContent = direction < 0 ? "−10 dtk" : "+10 dtk";
      seekFeedback.classList.toggle("is-left", direction < 0);
      seekFeedback.classList.toggle("is-right", direction > 0);
      seekFeedback.classList.add("is-visible");
      window.clearTimeout(seekFeedbackTimer);
      seekFeedbackTimer = window.setTimeout(() => seekFeedback.classList.remove("is-visible"), 650);
    };

    const showSpeedFeedback = (locked, text = "2×") => {
      if (!speedFeedback) return;
      speedFeedback.textContent = locked ? "2× terkunci" : text;
      speedFeedback.classList.toggle("is-locked", locked);
      speedFeedback.classList.add("is-visible");
    };

    const hideSpeedFeedback = () => {
      if (!speedFeedback || speedLocked) return;
      speedFeedback.classList.remove("is-visible", "is-locked");
    };

    const gestureZone = (clientX) => {
      const bounds = gestureLayer?.getBoundingClientRect();
      if (!bounds || bounds.width <= 0) return "center";
      const position = (clientX - bounds.left) / bounds.width;
      if (position < 0.34) return "left";
      if (position > 0.66) return "right";
      return "center";
    };

    const handleGestureDown = (event) => {
      if (event.pointerType !== "touch" && event.pointerType !== "pen") return;
      if (activeGesture) return;
      event.preventDefault();
      suppressTouchClick = true;

      const priorTap = pendingTap;
      if (priorTap) {
        window.clearTimeout(priorTap.timer);
        pendingTap = null;
      }

      activeGesture = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        zone: gestureZone(event.clientX),
        startedAt: Date.now(),
        lockedAtStart: speedLocked,
        priorTap,
        moved: false,
        held: false,
        holdTimer: 0,
      };

      activeGesture.holdTimer = window.setTimeout(() => {
        if (!activeGesture || activeGesture.moved) return;
        activeGesture.held = true;
        if (!activeGesture.lockedAtStart) setPlaybackRate(2);
        showSpeedFeedback(activeGesture.lockedAtStart, activeGesture.lockedAtStart ? "Geser ke atas" : "2×");
        showControls(false);
      }, 420);

      try {
        gestureLayer.setPointerCapture(event.pointerId);
      } catch (_) {}
      showControls(false);
    };

    const handleGestureMove = (event) => {
      const gesture = activeGesture;
      if (!gesture || event.pointerId !== gesture.pointerId) return;
      event.preventDefault();
      const deltaX = event.clientX - gesture.startX;
      const deltaY = event.clientY - gesture.startY;
      if (!gesture.held && (Math.abs(deltaX) > 18 || Math.abs(deltaY) > 18)) {
        gesture.moved = true;
        window.clearTimeout(gesture.holdTimer);
      }
      if (!gesture.held) return;

      if (!gesture.lockedAtStart && !speedLocked && deltaY > 58) {
        speedLocked = true;
        setPlaybackRate(2);
        showSpeedFeedback(true);
      } else if (gesture.lockedAtStart && speedLocked && deltaY < -58) {
        speedLocked = false;
        setPlaybackRate(configuredPlaybackRate);
        showSpeedFeedback(false, `${configuredPlaybackRate}×`);
        window.setTimeout(hideSpeedFeedback, 600);
      }
    };

    const finishGesture = (event, cancelled = false) => {
      const gesture = activeGesture;
      if (!gesture || (event && event.pointerId !== gesture.pointerId)) return;
      window.clearTimeout(gesture.holdTimer);
      activeGesture = null;

      if (gesture.held) {
        if (!speedLocked && !gesture.lockedAtStart) {
          setPlaybackRate(configuredPlaybackRate);
          hideSpeedFeedback();
        }
      } else if (!cancelled && !gesture.moved && Date.now() - gesture.startedAt < 360) {
        const now = Date.now();
        const prior = gesture.priorTap;
        if (prior && prior.zone === gesture.zone && now - prior.at <= 320) {
          if (gesture.zone === "left") {
            seekTo((video.currentTime || 0) - 10);
            showSeekFeedback(-1);
          } else if (gesture.zone === "right") {
            seekTo((video.currentTime || 0) + 10);
            showSeekFeedback(1);
          } else {
            togglePlay();
          }
        } else {
          if (prior) togglePlay();
          const tap = { zone: gesture.zone, at: now, timer: 0 };
          tap.timer = window.setTimeout(() => {
            if (pendingTap !== tap) return;
            pendingTap = null;
            togglePlay();
          }, 300);
          pendingTap = tap;
        }
      }

      window.setTimeout(() => {
        suppressTouchClick = false;
      }, 0);
      showControls();
    };

    gestureLayer?.addEventListener("pointerdown", handleGestureDown);
    gestureLayer?.addEventListener("pointermove", handleGestureMove);
    gestureLayer?.addEventListener("pointerup", (event) => finishGesture(event));
    gestureLayer?.addEventListener("pointercancel", (event) => finishGesture(event, true));
    gestureLayer?.addEventListener("contextmenu", (event) => event.preventDefault());

    /* --- Layar penuh + orientasi landscape ---------------------------- */

    const fullscreenElement = () =>
      document.fullscreenElement || document.webkitFullscreenElement || null;

    const isFullscreen = () => Boolean(fullscreenElement()) || pseudoFullscreen;

    const syncFullscreenIcon = () => {
      setIcon(fullscreenButton, isFullscreen() ? "i-compress" : "i-expand");
      fullscreenButton?.setAttribute(
        "aria-label",
        isFullscreen() ? "Keluar layar penuh" : "Layar penuh",
      );
    };

    const lockLandscape = () => {
      try {
        const promise = screen.orientation?.lock?.("landscape");
        promise?.catch?.(() => {});
      } catch (_) {}
    };

    const unlockOrientation = () => {
      try {
        screen.orientation?.unlock?.();
      } catch (_) {}
    };

    const enterPseudoFullscreen = () => {
      pseudoFullscreen = true;
      stage?.classList.add("is-pseudo-fullscreen");
      document.body.classList.add("player-fs-open");
      syncFullscreenIcon();
    };

    const exitPseudoFullscreen = () => {
      pseudoFullscreen = false;
      stage?.classList.remove("is-pseudo-fullscreen");
      document.body.classList.remove("player-fs-open");
      syncFullscreenIcon();
    };

    const enterFullscreen = () => {
      const element = stage || video;
      const request =
        element.requestFullscreen || element.webkitRequestFullscreen || element.msRequestFullscreen;
      if (!request) {
        enterPseudoFullscreen();
        lockLandscape();
        return;
      }
      let result;
      try {
        result = request.call(element);
      } catch (_) {
        enterPseudoFullscreen();
        lockLandscape();
        return;
      }
      if (result?.catch) {
        result.catch(() => {
          enterPseudoFullscreen();
          lockLandscape();
        });
      }
      lockLandscape();
      syncFullscreenIcon();
      showControls(false);
    };

    const exitFullscreen = () => {
      try {
        if (document.exitFullscreen && document.fullscreenElement) {
          document.exitFullscreen().catch(() => {});
        } else if (document.webkitExitFullscreen && document.webkitFullscreenElement) {
          document.webkitExitFullscreen();
        }
      } catch (_) {}
      exitPseudoFullscreen();
      unlockOrientation();
    };

    const toggleFullscreen = () => {
      if (isFullscreen()) exitFullscreen();
      else enterFullscreen();
      showControls(false);
    };

    const onFullscreenChange = () => {
      if (fullscreenElement()) {
        syncFullscreenIcon();
        showControls(false);
        lockLandscape();
      } else {
        if (!pseudoFullscreen) unlockOrientation();
        exitPseudoFullscreen();
        syncFullscreenIcon();
      }
    };

    document.addEventListener("fullscreenchange", onFullscreenChange);
    document.addEventListener("webkitfullscreenchange", onFullscreenChange);

    /* --- Events ------------------------------------------------------- */

    stage?.addEventListener("pointermove", () => showControls());
    stage?.addEventListener("pointerdown", () => showControls());
    stage?.addEventListener("pointerleave", () => {
      if (!video.paused) hideControlsSoon();
    });

    toggleButton?.addEventListener("click", (event) => {
      event.stopPropagation();
      togglePlay();
    });

    fullscreenButton?.addEventListener("click", (event) => {
      event.stopPropagation();
      toggleFullscreen();
    });

    scrub?.addEventListener("pointerdown", (event) => {
      if (!Number.isFinite(video.duration) || video.duration <= 0) return;
      event.preventDefault();
      event.stopPropagation();
      scrubbing = true;
      scrubWasPlaying = !video.paused && !video.ended;
      if (scrubWasPlaying) video.pause();
      stage?.classList.add("is-scrubbing");
      showControls(false);
      try {
        scrub.setPointerCapture(event.pointerId);
      } catch (_) {}
      paintPlayed(ratioFromPointer(event));
    });

    scrub?.addEventListener("pointermove", (event) => {
      if (!scrubbing) return;
      paintPlayed(ratioFromPointer(event));
    });

    const finishScrub = (event) => {
      if (!scrubbing) return;
      scrubbing = false;
      stage?.classList.remove("is-scrubbing");
      seekTo(ratioFromPointer(event) * (video.duration || 0));
      if (scrubWasPlaying) video.play().catch(() => {});
      showControls();
    };

    scrub?.addEventListener("pointerup", finishScrub);
    scrub?.addEventListener("pointercancel", finishScrub);

    scrub?.addEventListener("keydown", (event) => {
      if (event.key === "ArrowRight" || event.key === "ArrowUp") {
        event.preventDefault();
        event.stopPropagation();
        seekTo((video.currentTime || 0) + 5);
      } else if (event.key === "ArrowLeft" || event.key === "ArrowDown") {
        event.preventDefault();
        event.stopPropagation();
        seekTo((video.currentTime || 0) - 5);
      } else if (event.key === "Home") {
        event.preventDefault();
        event.stopPropagation();
        seekTo(0);
      } else if (event.key === "End") {
        event.preventDefault();
        event.stopPropagation();
        seekTo((video.duration || 0) - 0.5);
      } else if (event.key === " " || event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        togglePlay();
      }
    });

    video.addEventListener("play", () => {
      paintPlay();
      showControls();
    });
    video.addEventListener("playing", () => {
      closeEndScreen();
      paintPlay();
      showControls();
    });
    video.addEventListener("pause", () => {
      paintPlay();
      showControls(false);
    });
    video.addEventListener("ended", () => {
      paintPlay();
      showControls(false);
    });
    video.addEventListener("volumechange", paintMute);
    video.addEventListener("timeupdate", paintTime);
    video.addEventListener("progress", paintBuffer);
    video.addEventListener("loadedmetadata", paintTime);
    video.addEventListener("durationchange", paintTime);

    document.addEventListener("keydown", (event) => {
      if (event.defaultPrevented) return;
      if (event.target === scrub) return;
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest("button, a, input, textarea, select, [contenteditable='true']")
      ) {
        return;
      }
      if (event.key === "Escape" && pseudoFullscreen) {
        event.preventDefault();
        exitFullscreen();
        return;
      }
      if (!stage) return;

      const key = event.key.toLowerCase();
      if (key === "f") {
        event.preventDefault();
        toggleFullscreen();
      } else if (key === "m") {
        event.preventDefault();
        toggleMute();
      } else if (key === " " || key === "k") {
        event.preventDefault();
        togglePlay();
      } else if (key === "j" || event.key === "ArrowLeft") {
        event.preventDefault();
        showControls();
        seekTo((video.currentTime || 0) - 5);
      } else if (key === "l" || event.key === "ArrowRight") {
        event.preventDefault();
        showControls();
        seekTo((video.currentTime || 0) + 5);
      }
    });

    applyAudioSettings();
    setConfiguredPlaybackRate(configuredPlaybackRate);
    if (audioBoostControl && !window.AudioContext && !window.webkitAudioContext) {
      audioBoost = 1;
      writeValue(STORE.audioBoost, audioBoost);
      audioBoostControl.disabled = true;
      audioBoostControl.title = "Penguat suara tidak didukung browser ini";
    }
    paintMute();
    paintVolume();
    paintAudioBoost();
    paintPlaybackRate();
    paintAutoSettings();
    paintPlay();
    paintTime();
    showControls(false);

    const paintQuality = () => {
      qualityButtons.forEach((button) => {
        const value = button.dataset.quality;
        const active = value === "auto" ? mode === "auto" : mode === value;
        button.classList.toggle("is-active", active);
        button.classList.toggle("is-selected", active);
        button.setAttribute("aria-checked", active ? "true" : "false");
      });
      const text = group?.quality || (mode === "auto" ? "Otomatis" : mode);
      if (qualityToggle) {
        const selectedMode = mode === "auto" ? `Otomatis, ${text}` : text;
        qualityToggle.setAttribute("aria-label", `Pengaturan pemutar, resolusi ${selectedMode}`);
      }
    };

    const connectionInfo = () => {
      const connection =
        navigator.connection || navigator.mozConnection || navigator.webkitConnection || null;
      return {
        effective: connection?.effectiveType || "",
        downlink: Number(connection?.downlink) || 0,
        saveData: Boolean(connection?.saveData),
      };
    };

    const pickAutoGroup = () => {
      if (groups.length === 0) return null;
      const info = connectionInfo();
      const ranks = groups.map((entry) => entry.rank).sort((a, b) => a - b);
      const lowest = ranks[0];
      let target = 720;
      if (info.saveData || info.effective === "slow-2g" || info.effective === "2g") target = 360;
      else if (info.effective === "3g") target = 480;
      else if (info.downlink && info.downlink < 2.5) target = 480;
      else if (info.downlink >= 15) target = 1080;
      else if (info.downlink >= 7) target = 720;

      const below = groups.find((entry) => entry.rank <= target);
      if (below) return below;
      return groups[groups.length - 1] || groups.find((entry) => entry.rank === lowest);
    };

    const applyMode = (nextMode, playNow = false) => {
      mode = nextMode;
      writeValue(STORE.quality, mode);
      if (mode === "auto") {
        group = pickAutoGroup();
      } else {
        group = groups.find((entry) => entry.quality === mode) || group || pickAutoGroup();
      }
      serverIndex = 0;
      paintQuality();
      loadCurrent(playNow);
    };

    const loadCurrent = (playNow = false, seekTo = null) => {
      if (!group || group.servers.length === 0) {
        setStatus("Tidak ada sumber yang bisa diputar", true);
        if (bigPlay) bigPlay.classList.remove("is-hidden");
        return;
      }
      const server = group.servers[serverIndex];
      if (!server) return;

      const previousTime = seekTo ?? (video.currentTime || 0);
      const wasPlaying = playNow || (!video.paused && started);
      pausedBySwitch = wasPlaying;
      switching = true;
      paintQuality();
      setStatus(`Memuat ${group.quality}…`);

      const sourceUrl = server.play_url || server.url;
      if (video.getAttribute("src") !== sourceUrl) {
        video.src = sourceUrl;
        video.load();
        setPlaybackRate(speedLocked ? 2 : configuredPlaybackRate);
      }

      const resumeAt = () => {
        if (previousTime > 1 && Number.isFinite(previousTime)) {
          try {
            video.currentTime = previousTime;
          } catch (_) {}
        }
        if (wasPlaying) {
          video.play().catch(() => {
            if (bigPlay) bigPlay.classList.remove("is-hidden");
          });
        }
      };

      if (video.readyState >= 1) {
        resumeAt();
      } else {
        video.addEventListener("loadedmetadata", resumeAt, { once: true });
      }
    };

    const tryNextSource = () => {
      if (!group) return;
      const current = group.servers[serverIndex];
      if (current?.url) dead.add(current.url);

      const nextServer = group.servers.findIndex(
        (server, index) => index > serverIndex && !dead.has(server.url),
      );
      if (nextServer >= 0) {
        serverIndex = nextServer;
        loadCurrent(true);
        toast("Server diganti otomatis", "i-server");
        return;
      }

      if (mode === "auto") {
        const lowerIndex = groups.indexOf(group);
        const lower = groups
          .slice(lowerIndex + 1)
          .find((entry) => entry.servers.some((server) => !dead.has(server.url)));
        if (lower) {
          group = lower;
          serverIndex = 0;
          paintQuality();
          lastQualityChange = Date.now();
          loadCurrent(true);
          setStatus(`Turun ke ${lower.quality}`, true);
          toast(`Kualitas diturunkan ke ${lower.quality}`, "i-info");
          return;
        }
      }

      setStatus("Sumber ini tidak dapat dibuka", true);
      if (bigPlay) bigPlay.classList.remove("is-hidden");
      toast("Sumber video tidak dapat dibuka", "i-info");
    };

    const saveProgress = (force = false) => {
      if (!slug || !Number.isFinite(video.duration) || video.duration <= 0) return;
      if (!force && Date.now() - lastSaveAt < 5000) return;
      lastSaveAt = Date.now();
      const position = video.currentTime || 0;
      const percent = Math.max(0, Math.min(100, Math.round((position / video.duration) * 100)));
      writeValue(progressKey, { position, duration: video.duration, at: Date.now() });
      updateHistoryEntry(slug, { progress: percent, position });
    };

    const startPlayback = () => {
      started = true;
      ensureAudioGraph();
      if (!group) group = mode === "auto" ? pickAutoGroup() : groups.find((entry) => entry.quality === mode) || pickAutoGroup();
      if (!group) {
        setStatus("Tidak ada sumber yang bisa diputar", true);
        return;
      }
      if (bigPlay) bigPlay.classList.add("is-hidden");
      if (!video.src) {
        loadCurrent(true);
      } else {
        if (video.ended) {
          try {
            video.currentTime = 0;
          } catch (_) {}
        }
        video.play().catch(() => {
          if (bigPlay) bigPlay.classList.remove("is-hidden");
        });
      }
    };

    if (groups.length === 0) {
      setStatus("Tidak ada sumber yang bisa diputar", true);
      if (bigPlay) bigPlay.classList.add("is-hidden");
    } else {
      group = mode === "auto" ? pickAutoGroup() : groups.find((entry) => entry.quality === mode) || pickAutoGroup();
      paintQuality();
      setStatus(`Siap diputar · ${mode === "auto" ? "Otomatis" : group?.quality || mode}`);
    }

    bigPlay?.addEventListener("click", startPlayback);
    let suppressTouchClick = false;
    gestureLayer?.addEventListener("click", (event) => {
      if (suppressTouchClick || event.pointerType === "touch" || event.pointerType === "pen") return;
      if (!started) {
        startPlayback();
        return;
      }
      togglePlay();
    });
    gestureLayer?.addEventListener("dblclick", (event) => {
      if (event.pointerType === "touch" || event.pointerType === "pen") return;
      event.preventDefault();
      toggleFullscreen();
    });

    const closeQualityMenu = (restoreFocus = false) => {
      if (!qualityMenu || !qualityToggle) return;
      qualityMenu.hidden = true;
      qualityMenu.classList.remove("opens-down", "opens-up");
      qualityMenu.style.maxHeight = "";
      qualityToggle.setAttribute("aria-expanded", "false");
      qualityControl?.classList.remove("is-open");
      showControls();
      if (restoreFocus) qualityToggle.focus();
    };

    const positionQualityMenu = () => {
      if (!qualityMenu || !qualityToggle || qualityMenu.hidden) return;

      const visualViewport = window.visualViewport;
      const viewportTop = visualViewport?.offsetTop || 0;
      const viewportBottom = viewportTop + (visualViewport?.height || window.innerHeight);
      let topLimit = viewportTop + 8;
      let bottomLimit = viewportBottom - 8;
      const header = $(".topbar");
      const headerRect = header?.getBoundingClientRect();
      if (headerRect && headerRect.top <= viewportBottom && headerRect.bottom > viewportTop) {
        topLimit = Math.max(topLimit, headerRect.bottom + 6);
      }
      const bottomNav = $(".bottom-nav");
      const bottomNavRect = bottomNav?.getBoundingClientRect();
      if (
        bottomNav &&
        getComputedStyle(bottomNav).display !== "none" &&
        bottomNavRect.top < viewportBottom &&
        bottomNavRect.bottom > viewportTop
      ) {
        bottomLimit = Math.min(bottomLimit, bottomNavRect.top - 6);
      }

      qualityMenu.style.maxHeight = "none";
      const menuHeight = qualityMenu.getBoundingClientRect().height;
      const toggleRect = qualityToggle.getBoundingClientRect();
      const spaceAbove = Math.max(0, toggleRect.top - topLimit - 8);
      const spaceBelow = Math.max(0, bottomLimit - toggleRect.bottom - 8);
      const opensDown = menuHeight > spaceAbove && (menuHeight <= spaceBelow || spaceBelow > spaceAbove);
      const availableSpace = opensDown ? spaceBelow : spaceAbove;

      qualityMenu.classList.toggle("opens-down", opensDown);
      qualityMenu.classList.toggle("opens-up", !opensDown);
      qualityMenu.style.maxHeight = `${Math.max(0, Math.min(menuHeight, availableSpace))}px`;
    };

    window.addEventListener("resize", positionQualityMenu, { passive: true });
    window.visualViewport?.addEventListener("resize", positionQualityMenu, { passive: true });
    window.addEventListener("scroll", positionQualityMenu, { passive: true });

    const closeVolumePopover = (restoreFocus = false) => {
      if (!volumePopover || !muteButton || volumePopover.hidden) return;
      volumePopover.hidden = true;
      muteButton.setAttribute("aria-expanded", "false");
      volumeControlPanel?.classList.remove("is-open");
      showControls();
      if (restoreFocus) muteButton.focus();
    };

    const openVolumePopover = () => {
      if (!volumePopover || !muteButton) return;
      closeQualityMenu();
      volumePopover.hidden = false;
      muteButton.setAttribute("aria-expanded", "true");
      volumeControlPanel?.classList.add("is-open");
      showControls(false);
    };

    const openQualityMenu = () => {
      if (!qualityMenu || !qualityToggle) return;
      closeVolumePopover();
      qualityMenu.hidden = false;
      qualityToggle.setAttribute("aria-expanded", "true");
      qualityControl?.classList.add("is-open");
      qualityMenu.scrollTop = 0;
      showControls(false);
      positionQualityMenu();
    };

    muteButton?.addEventListener("click", (event) => {
      event.stopPropagation();
      toggleMute();
      openVolumePopover();
    });

    qualityToggle?.addEventListener("click", () => {
      if (qualityMenu?.hidden) openQualityMenu();
      else closeQualityMenu();
    });

    qualityButtons.forEach((button) => {
      button.addEventListener("click", () => {
        const value = button.dataset.quality;
        if (value !== "auto" && !groups.some((entry) => entry.quality === value)) return;
        applyMode(value, started);
        closeQualityMenu(true);
        if (value !== "auto") toast(`Resolusi ${value}`, "i-server");
      });
    });

    playbackRateButtons.forEach((button) => {
      button.addEventListener("click", () => {
        setConfiguredPlaybackRate(button.dataset.playbackRate);
        speedLocked = false;
        writeValue(STORE.playbackRate, configuredPlaybackRate);
        paintPlaybackRate();
        if (speedFeedback) speedFeedback.classList.remove("is-visible", "is-locked");
        showControls(false);
      });
    });

    autoReplayControl?.addEventListener("change", () => {
      autoReplay = autoReplayControl.checked;
      if (autoReplay) autoContinue = false;
      writeValue(STORE.autoReplay, autoReplay);
      writeValue(STORE.autoContinue, autoContinue);
      paintAutoSettings();
      if (autoReplay && video.ended) {
        closeQualityMenu();
        replayEpisode();
      }
    });

    autoContinueControl?.addEventListener("change", () => {
      autoContinue = Boolean(nextUrl) && autoContinueControl.checked;
      if (autoContinue) autoReplay = false;
      writeValue(STORE.autoReplay, autoReplay);
      writeValue(STORE.autoContinue, autoContinue);
      paintAutoSettings();
      if (autoContinue && video.ended) window.location.assign(nextUrl);
    });

    endReplayButton?.addEventListener("click", replayEpisode);

    volumeControl?.addEventListener("input", () => {
      currentVolume = clampNumber(volumeControl.value, 0, 100, 80) / 100;
      if (currentVolume > 0) {
        previousVolume = currentVolume;
        userMuted = false;
      } else {
        userMuted = true;
      }
      writeValue(STORE.volume, currentVolume);
      applyAudioSettings();
      paintVolume();
      showControls();
    });

    audioBoostControl?.addEventListener("input", () => {
      const nextBoost = clampNumber(audioBoostControl.value, 100, 200, 100) / 100;
      if (nextBoost > 1 && !ensureAudioGraph()) {
        audioBoost = 1;
        writeValue(STORE.audioBoost, audioBoost);
        paintAudioBoost();
        toast("Penguat suara tidak didukung browser ini", "i-info");
        return;
      }
      audioBoost = nextBoost;
      writeValue(STORE.audioBoost, audioBoost);
      applyAudioSettings();
      paintAudioBoost();
      showControls(false);
    });

    reportButton?.addEventListener("click", async () => {
      const reportText = `Laporan masalah pemutaran: ${document.title}\n${window.location.href}`;
      closeQualityMenu();
      if (navigator.share) {
        try {
          await navigator.share({ title: "Laporkan masalah pemutaran", text: reportText });
          return;
        } catch (error) {
          if (error?.name === "AbortError") return;
        }
      }
      try {
        await navigator.clipboard.writeText(reportText);
        toast("Detail laporan disalin. Kirim ke pengelola anime.", "i-check");
      } catch (_) {
        window.prompt("Salin detail laporan ini", reportText);
      }
    });

    document.addEventListener("click", (event) => {
      if (qualityControl && !qualityControl.contains(event.target)) closeQualityMenu();
      if (volumeControlPanel && !volumeControlPanel.contains(event.target)) closeVolumePopover();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        if (qualityMenu && !qualityMenu.hidden) closeQualityMenu(true);
        if (volumePopover && !volumePopover.hidden) closeVolumePopover(true);
      } else if (
        event.key === "ArrowDown" &&
        document.activeElement === qualityToggle &&
        qualityMenu?.hidden
      ) {
        event.preventDefault();
        openQualityMenu();
        qualityButtons[0]?.focus();
      }
    });

    video.addEventListener("loadedmetadata", () => {
      setPlaybackRate(speedLocked ? 2 : configuredPlaybackRate);
      if (!resumeApplied) {
        resumeApplied = true;
        const saved = readValue(progressKey, null);
        if (
          saved &&
          Number(saved.position) > 10 &&
          Number(saved.duration) > 0 &&
          Number(saved.position) < Number(saved.duration) - 15
        ) {
          try {
            video.currentTime = Number(saved.position);
            setStatus(`Melanjutkan dari ${formatDuration(saved.position)}`);
          } catch (_) {}
        }
      }
    });

    video.addEventListener("playing", () => {
      switching = false;
      stallCount = 0;
      if (bigPlay) bigPlay.classList.add("is-hidden");
      if (group) {
        setStatus(`${group.quality} · Sedang diputar`, false, true);
        paintQuality();
      }
      if (!healthySince) healthySince = Date.now();
    });

    video.addEventListener("waiting", () => {
      if (!group) return;
      setStatus("Buffering…");
      const now = Date.now();
      if (now - lastStallAt > 25000) stallCount = 0;
      lastStallAt = now;
      stallCount += 1;

      if (mode === "auto" && stallCount >= 2 && now - lastQualityChange > 8000) {
        const index = groups.indexOf(group);
        const lower = groups[index + 1];
        if (lower) {
          group = lower;
          serverIndex = 0;
          lastQualityChange = now;
          stallCount = 0;
          paintQuality();
          loadCurrent(true);
          toast(`Internet lambat, turun ke ${lower.quality}`, "i-info");
        }
      }
    });

    video.addEventListener("stalled", () => {
      setStatus("Koneksi lambat…", true);
    });

    video.addEventListener("timeupdate", () => {
      saveProgress(false);
      if (mode !== "auto" || !group) return;
      const index = groups.indexOf(group);
      if (index <= 0) return;

      let buffered = 0;
      try {
        buffered = (video.buffered.end(video.buffered.length - 1) || 0) - video.currentTime;
      } catch (_) {
        buffered = 0;
      }

      if (buffered > 12) {
        if (!upgradeArmed) {
          upgradeArmed = true;
          healthySince = Date.now();
        } else if (Date.now() - healthySince > 12000) {
          const higher = groups[index - 1];
          if (higher) {
            group = higher;
            serverIndex = 0;
            lastQualityChange = Date.now();
            upgradeArmed = false;
            paintQuality();
            loadCurrent(true);
            toast(`Kualitas dinaikkan ke ${higher.quality}`, "i-server");
          }
        }
      } else {
        upgradeArmed = false;
        healthySince = Date.now();
      }
    });

    video.addEventListener("pause", () => {
      saveProgress(true);
    });

    video.addEventListener("error", () => {
      if (switching) return;
      setStatus("Sumber gagal dimuat", true);
      tryNextSource();
    });

    video.addEventListener("ended", () => {
      saveProgress(true);
      writeValue(progressKey, { position: 0, duration: video.duration || 0, at: Date.now() });
      updateHistoryEntry(slug, { progress: 100 });
      setStatus("Selesai");
      closeQualityMenu();
      closeVolumePopover();
      bigPlay?.classList.add("is-hidden");
      if (autoContinue && nextUrl) {
        window.location.assign(nextUrl);
        return;
      }
      if (autoReplay) {
        replayEpisode();
        return;
      }
      if (endScreen) {
        endScreen.hidden = false;
        stage?.classList.add("is-ended");
      }
      showControls(false);
    });

    navigator.connection?.addEventListener?.("change", () => {
      if (mode !== "auto" || !started) return;
      const target = pickAutoGroup();
      if (target && target !== group && Date.now() - lastQualityChange > 15000) {
        group = target;
        serverIndex = 0;
        lastQualityChange = Date.now();
        paintQuality();
        loadCurrent(true);
        toast(`Jaringan berubah, kualitas ${target.quality}`, "i-info");
      }
    });

    if (historyNode) {
      recordHistory({
        slug: historyNode.dataset.slug,
        title: historyNode.dataset.title,
        image: historyNode.dataset.image,
        episode: historyNode.dataset.episode,
        href: historyNode.dataset.href,
      });
    }
  })();

  /* ---------------------------------------------------------
     Tombol bookmark global
     --------------------------------------------------------- */

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-bookmark]");
    if (!button) return;
    event.preventDefault();
    toggleBookmark({
      slug: button.dataset.slug,
      title: button.dataset.title,
      image: button.dataset.image,
      href: button.dataset.href,
    });
    paintBookmarkButtons();
    if ($("#bookmark-grid")) renderBookmarkPage();
  });

  /* ---------------------------------------------------------
     Init
     --------------------------------------------------------- */

  applyImageFallbacks();
  makeWatchTitleDisplayOnly();
  paintBookmarkButtons();
  renderContinueWatching();
  renderBookmarkPage();
  initAccountPage();
  initHistoryPage();
  initDownloadCollection();
  initEpisodeDownloadList();
  initOfflineCollection();
  initComments();
})();
