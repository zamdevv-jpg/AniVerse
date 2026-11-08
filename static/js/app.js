/* =========================================================
   AniVerse — interaksi klien
   Semua data pribadi (bookmark & riwayat) disimpan di localStorage.
   ========================================================= */
(() => {
  "use strict";

  const STORE = {
    bookmarks: "otakunime:bookmarks",
    history: "otakunime:history",
    downloads: "aniverse:downloads",
    offline: "aniverse:offline-pages",
    accounts: "aniverse:accounts",
    session: "aniverse:session",
  };

  const OFFLINE_PAGE_CACHE = "aniverse-offline-pages-v1";

  const $ = (selector, scope = document) => scope.querySelector(selector);
  const $$ = (selector, scope = document) => Array.from(scope.querySelectorAll(selector));

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

  // Thumbnail kecil untuk dropdown pencarian (hemat kuota, tetap tajam).
  const thumbUrl = (url) => String(url ?? "").replace(/resize=\d+,\d+/, "resize=120,180");

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
      setTimeout(() => node.remove(), 220);
    }, 2200);
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
    return writeStore(STORE.bookmarks, list.slice(0, 120));
  }

  function removeBookmark(slug) {
    return writeStore(
      STORE.bookmarks,
      getBookmarks().filter((item) => item.slug !== slug),
    );
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
          <span class="badge badge-type">Bookmark</span>
          <span class="poster-play" aria-hidden="true"><svg class="icon fill"><use href="#i-play"/></svg></span>
        </div>
        <div class="poster-body">
          <h3 class="poster-title">${escapeHtml(item.title)}</h3>
          <div class="poster-meta"><span>Disimpan</span></div>
        </div>
      </a>`;
  }

  function renderBookmarkPage() {
    const grid = $("#bookmark-grid");
    const empty = $("#bookmark-empty");
    if (!grid) return;

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
    const list = getHistory().filter((item) => item.slug !== data.slug);
    list.unshift({
      slug: data.slug,
      title: data.title || "Episode",
      image: data.image || "",
      episode: data.episode || "",
      href: data.href || `/watch/${encodeURIComponent(data.slug)}`,
      watchedAt: Date.now(),
    });
    writeStore(STORE.history, list.slice(0, 40));
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
        return `
          <a class="poster-card" href="${escapeHtml(item.href)}" title="${escapeHtml(item.title)}">
            <div class="poster">
              ${image}
              <span class="poster-shade" aria-hidden="true"></span>
              ${item.episode ? `<span class="badge badge-ep">Ep ${escapeHtml(item.episode)}</span>` : ""}
              <span class="poster-play" aria-hidden="true"><svg class="icon fill"><use href="#i-play"/></svg></span>
            </div>
            <div class="poster-body">
              <h3 class="poster-title">${escapeHtml(item.title)}</h3>
              <div class="poster-meta"><span>Lanjut tonton</span></div>
            </div>
          </a>`;
      })
      .join("");
    applyImageFallbacks(grid);
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
     Header
     --------------------------------------------------------- */

  const header = $("#site-header");
  const mobileMenuToggle = $("#mobile-menu-toggle");
  const mobileSearchToggle = $("#mobile-search-toggle");
  const mobileSearchBar = $("#mobile-search-bar");

  function closeMobileMenu() {
    if (!header) return;
    header.classList.remove("is-open");
    if (mobileMenuToggle) {
      mobileMenuToggle.setAttribute("aria-expanded", "false");
      const use = $("use", mobileMenuToggle);
      if (use) use.setAttribute("href", "#i-menu");
    }
  }

  if (header) {
    const onScroll = () => {
      header.classList.toggle("is-scrolled", window.scrollY > 8);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
  }

  if (mobileMenuToggle && header) {
    mobileMenuToggle.addEventListener("click", () => {
      const open = header.classList.toggle("is-open");
      mobileMenuToggle.setAttribute("aria-expanded", open ? "true" : "false");
      const use = $("use", mobileMenuToggle);
      if (use) use.setAttribute("href", open ? "#i-x" : "#i-menu");
      if (open && mobileSearchBar) mobileSearchBar.hidden = true;
    });
    header.addEventListener("click", (event) => {
      if (event.target.closest(".desktop-nav a")) closeMobileMenu();
    });
    document.addEventListener("click", (event) => {
      if (!header.contains(event.target)) closeMobileMenu();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") closeMobileMenu();
    });
  }

  if (mobileSearchToggle && mobileSearchBar) {
    mobileSearchToggle.addEventListener("click", () => {
      const willShow = mobileSearchBar.hidden;
      mobileSearchBar.hidden = !willShow;
      mobileSearchToggle.setAttribute("aria-expanded", willShow ? "true" : "false");
      if (willShow) {
        closeMobileMenu();
        const input = $("input", mobileSearchBar);
        if (input) setTimeout(() => input.focus(), 60);
      }
    });
  }

  /* ---------------------------------------------------------
     Pencarian langsung
     --------------------------------------------------------- */

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
      const nodes = $$(".search-item", searchDropdown);
      nodes.forEach((node, index) => {
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
              ? `<img src="${escapeHtml(thumbUrl(item.image))}" alt="" decoding="async">`
              : `<span class="poster-fallback" aria-hidden="true" style="font-size:16px">${escapeHtml(initials(item.title))}</span>`;
            return `
              <a class="search-item" href="${escapeHtml(item.href)}" data-index="${index}">
                <span class="search-item-img">${image}</span>
                <span class="search-item-body">
                  <strong>${escapeHtml(item.title)}</strong>
                  <span>${escapeHtml([item.episode, item.status].filter(Boolean).join(" · ") || "Anime")}</span>
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
      if (lastQuery.length >= 2 && !searchDropdown.hidden) return;
      if (lastQuery.length >= 2) runSearch(lastQuery);
    });

    searchForm.addEventListener("submit", (event) => {
      if (selectedIndex >= 0 && items[selectedIndex]) {
        event.preventDefault();
        window.location.href = items[selectedIndex].href;
      }
    });

    searchInput.addEventListener("keydown", (event) => {
      const nodes = items.length;
      if (event.key === "ArrowDown" && nodes > 0) {
        event.preventDefault();
        selectedIndex = (selectedIndex + 1) % nodes;
        paintSelection();
      } else if (event.key === "ArrowUp" && nodes > 0) {
        event.preventDefault();
        selectedIndex = (selectedIndex - 1 + nodes) % nodes;
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
      slides.forEach((slide, i) => slide.classList.toggle("is-active", i === index));
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

  /* ---------------------------------------------------------
     Akun lokal, komentar, dan koleksi
     --------------------------------------------------------- */

  function currentAccount() {
    const email = readValue(STORE.session, "");
    if (!email) return null;
    const accounts = readValue(STORE.accounts, {});
    return accounts[email] || null;
  }

  function encodeBytes(bytes) {
    return btoa(String.fromCharCode(...new Uint8Array(bytes)));
  }

  function decodeBytes(value) {
    return Uint8Array.from(atob(value), (char) => char.charCodeAt(0));
  }

  async function passwordDigest(password, salt) {
    const material = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(password),
      "PBKDF2",
      false,
      ["deriveBits"],
    );
    return crypto.subtle.deriveBits(
      { name: "PBKDF2", salt, iterations: 210000, hash: "SHA-256" },
      material,
      256,
    );
  }

  function showAuthMode(mode) {
    const login = $("#login-form");
    const register = $("#register-form");
    if (!login || !register) return;
    const creating = mode === "register";
    login.hidden = creating;
    register.hidden = !creating;
    const title = $("#auth-title");
    const subtitle = $("#auth-subtitle");
    if (title) title.textContent = creating ? "Buat akun lokal" : "Masuk ke AniVerse";
    if (subtitle) subtitle.textContent = creating
      ? "Pilih nama dan kata sandi untuk profil di perangkat ini."
      : "Masuk untuk melihat histori dan menulis komentar di perangkat ini.";
  }

  function updateAccountStats() {
    const historyCount = $("#profile-history-count");
    const commentCount = $("#profile-comment-count");
    const bookmarkCount = $("#profile-bookmark-count");
    if (historyCount) historyCount.textContent = String(getHistory().length);
    if (commentCount) commentCount.textContent = String(getLocalComments().length);
    if (bookmarkCount) bookmarkCount.textContent = String(getBookmarks().length);
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
    return comments.sort((a, b) => Number(b.createdAt) - Number(a.createdAt));
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
    list.innerHTML = entries.slice(0, 16).map(({ kind, item }) => kind === "history"
      ? `<a class="profile-activity-item" href="${escapeHtml(item.href)}"><span class="activity-icon"><svg class="icon"><use href="#i-clock"/></svg></span><span><strong>${escapeHtml(item.title)}</strong><small>Menonton episode ${escapeHtml(item.episode || "—")} · ${escapeHtml(relativeTime(item.watchedAt))}</small></span><svg class="icon activity-arrow"><use href="#i-chevron-right"/></svg></a>`
      : `<a class="profile-activity-item" href="/watch/${encodeURIComponent(item.episodeSlug)}"><span class="activity-icon"><svg class="icon"><use href="#i-message"/></svg></span><span><strong>Berkomentar · ${escapeHtml(item.episodeTitle || "Episode anime")}</strong><small>${escapeHtml(item.text)} · ${escapeHtml(relativeTime(item.createdAt))}</small></span><svg class="icon activity-arrow"><use href="#i-chevron-right"/></svg></a>`
    ).join("");
    if (entries.length === 0) list.innerHTML = `<p class="profile-activity-empty">Aktivitas akan muncul di sini.</p>`;
  }

  function initAccountPage() {
    const shell = $("#account-auth");
    const profile = $("#account-profile");
    if (!shell || !profile) return;

    const account = currentAccount();
    if (account) {
      shell.hidden = true;
      profile.hidden = false;
      $("#profile-name").textContent = account.name;
      $("#profile-email").textContent = account.email;
      $("#profile-avatar").textContent = initials(account.name);
      updateAccountStats();
      renderAccountActivity();
    }

    $$('[data-account-tab]').forEach((button) => {
      button.addEventListener("click", () => {
        $$('[data-account-tab]').forEach((tab) => tab.classList.toggle("is-active", tab === button));
        renderAccountActivity(button.dataset.accountTab);
      });
    });
    $$('[data-account-tab-link]').forEach((link) => {
      link.addEventListener("click", () => {
        const tab = link.dataset.accountTabLink;
        $$('[data-account-tab]').forEach((button) => button.classList.toggle("is-active", button.dataset.accountTab === tab));
        renderAccountActivity(tab);
      });
    });

    $$('[data-auth-switch]').forEach((button) => {
      button.addEventListener("click", () => showAuthMode(button.dataset.authSwitch));
    });

    $("#register-form")?.addEventListener("submit", async (event) => {
      event.preventDefault();
      if (!crypto?.subtle) return toast("Perangkat belum mendukung pembuatan akun aman", "i-info");
      const form = new FormData(event.currentTarget);
      const name = String(form.get("name") || "").trim();
      const email = String(form.get("email") || "").trim().toLowerCase();
      const password = String(form.get("password") || "");
      if (password.length < 8) return toast("Kata sandi minimal 8 karakter", "i-info");
      const accounts = readValue(STORE.accounts, {});
      if (accounts[email]) return toast("Email sudah terdaftar di perangkat ini", "i-info");
      const salt = crypto.getRandomValues(new Uint8Array(16));
      try {
        const digest = await passwordDigest(password, salt);
        accounts[email] = { name, email, salt: encodeBytes(salt), digest: encodeBytes(digest), createdAt: Date.now() };
        if (!writeValue(STORE.accounts, accounts)) return toast("Penyimpanan perangkat penuh", "i-info");
        writeValue(STORE.session, email);
        toast("Akun lokal berhasil dibuat");
        window.location.reload();
      } catch {
        toast("Akun belum dapat dibuat di perangkat ini", "i-info");
      }
    });

    $("#login-form")?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const form = new FormData(event.currentTarget);
      const email = String(form.get("email") || "").trim().toLowerCase();
      const password = String(form.get("password") || "");
      const accountRecord = readValue(STORE.accounts, {})[email];
      if (!accountRecord || !crypto?.subtle) return toast("Akun atau kata sandi tidak cocok", "i-info");
      try {
        const digest = await passwordDigest(password, decodeBytes(accountRecord.salt));
        if (encodeBytes(digest) !== accountRecord.digest) return toast("Akun atau kata sandi tidak cocok", "i-info");
        writeValue(STORE.session, email);
        toast("Berhasil masuk");
        window.location.reload();
      } catch {
        toast("Tidak bisa memeriksa kata sandi", "i-info");
      }
    });

    $("#logout-button")?.addEventListener("click", () => {
      localStorage.removeItem(STORE.session);
      toast("Kamu sudah keluar", "i-user");
      window.location.reload();
    });
  }

  function relativeTime(timestamp) {
    const minutes = Math.max(1, Math.floor((Date.now() - Number(timestamp || Date.now())) / 60000));
    if (minutes < 60) return `${minutes} menit lalu`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} jam lalu`;
    const days = Math.floor(hours / 24);
    return `${days} hari lalu`;
  }

  function initHistoryPage() {
    const list = $("#history-list");
    if (!list) return;
    const items = getHistory();
    const empty = $("#history-empty");
    if (empty) empty.hidden = items.length > 0;
    list.innerHTML = items.map((item) => `
      <a class="history-item" href="${escapeHtml(item.href)}">
        <span class="history-poster">${item.image ? `<img src="${escapeHtml(item.image)}" alt="" loading="lazy">` : `<b>${escapeHtml(initials(item.title))}</b>`}</span>
        <span class="history-copy"><strong>${escapeHtml(item.title)}</strong><span>Episode ${escapeHtml(item.episode || "—")} · ${escapeHtml(relativeTime(item.watchedAt))}</span><small>Lanjutkan menonton <svg class="icon"><use href="#i-chevron-right"/></svg></small></span>
      </a>`).join("");
    applyImageFallbacks(list);
    $("#clear-history")?.addEventListener("click", () => {
      writeStore(STORE.history, []);
      list.replaceChildren();
      if (empty) empty.hidden = false;
      updateAccountStats();
      toast("Histori tontonan dihapus", "i-trash");
    });
  }

  function initDownloadCollection() {
    const list = $("#saved-download-list");
    if (!list) return;
    const empty = $("#downloads-empty");
    const render = () => {
      const items = readStore(STORE.downloads);
      if (empty) empty.hidden = items.length > 0;
      list.innerHTML = items.map((item, index) => `
        <article class="saved-download-item"><span class="download-file-icon"><svg class="icon"><use href="#i-download"/></svg></span><div><strong>${escapeHtml(item.title)}</strong><span>${escapeHtml(item.server)}${item.quality ? ` · ${escapeHtml(item.quality)}` : ""}</span><small>Ditambahkan ${escapeHtml(relativeTime(item.savedAt))}</small></div><a class="btn btn-gold btn-sm" href="${escapeHtml(item.url)}" target="_blank" rel="noopener noreferrer nofollow">Buka</a><button class="icon-btn remove-download" type="button" data-remove-download="${index}" aria-label="Hapus tautan"><svg class="icon"><use href="#i-x"/></svg></button></article>`).join("");
    };
    render();
    list.addEventListener("click", (event) => {
      const button = event.target.closest("[data-remove-download]");
      if (!button) return;
      const items = readStore(STORE.downloads);
      items.splice(Number(button.dataset.removeDownload), 1);
      writeStore(STORE.downloads, items);
      render();
      updateAccountStats();
      toast("Tautan dihapus", "i-trash");
    });
  }

  function initOfflineCollection() {
    const grid = $("#offline-grid");
    if (!grid) return;
    const empty = $("#offline-empty");
    const render = () => {
      const items = readStore(STORE.offline);
      if (empty) empty.hidden = items.length > 0;
      grid.innerHTML = items.map((item) => `
        <article class="offline-card"><a class="poster-card" href="${escapeHtml(item.href)}"><div class="poster">${item.image ? `<img src="${escapeHtml(item.image)}" alt="${escapeHtml(item.title)}" loading="lazy">` : `<span class="poster-fallback">${escapeHtml(initials(item.title))}</span>`}<span class="badge badge-type">Offline</span></div><div class="poster-body"><h3 class="poster-title">${escapeHtml(item.title)}</h3><div class="poster-meta"><span>Disimpan ${escapeHtml(relativeTime(item.savedAt))}</span></div></div></a><button type="button" class="text-btn offline-remove" data-remove-offline="${escapeHtml(item.href)}">Hapus simpanan</button></article>`).join("");
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

  function initComments() {
    const panel = $("#comments-panel");
    if (!panel) return;
    const slug = panel.dataset.commentSlug;
    const key = `aniverse:comments:${slug}`;
    const form = $("#comment-form");
    const gate = $("#comment-gate");
    const list = $("#comment-list");
    let sort = "top";

    function getComments() { return readStore(key); }
    function saveComments(items) { writeStore(key, items.slice(0, 300)); }
    function render() {
      const account = currentAccount();
      if (gate) gate.hidden = Boolean(account);
      if (form) form.hidden = !account;
      const author = $("#comment-author");
      if (author) author.textContent = account?.name || "Penonton";
      const items = getComments();
      const count = $("#comment-count");
      if (count) count.textContent = `${items.length} komentar`;
      const likes = readStore(`aniverse:comment-likes:${slug}`);
      const ordered = [...items].sort((a, b) => sort === "top"
        ? (Number(b.likes || 0) - Number(a.likes || 0)) || (b.createdAt - a.createdAt)
        : b.createdAt - a.createdAt);
      list.innerHTML = ordered.map((item) => `
        <article class="comment-item" id="comment-${escapeHtml(item.id)}"><span class="comment-avatar">${escapeHtml(initials(item.author))}</span><div class="comment-body"><div class="comment-meta"><strong>${escapeHtml(item.author)}</strong><span>${escapeHtml(relativeTime(item.createdAt))}</span></div><p>${escapeHtml(item.text)}</p><div class="comment-actions"><button type="button" data-comment-like="${escapeHtml(item.id)}" class="${likes.includes(item.id) ? "is-liked" : ""}">♥ ${Number(item.likes || 0)}</button><button type="button" data-comment-reply-toggle="${escapeHtml(item.id)}">Balas${item.replies?.length ? ` · ${item.replies.length}` : ""}</button></div><form class="reply-form" data-reply-form="${escapeHtml(item.id)}" hidden><input name="reply" maxlength="300" placeholder="Tulis balasan..." required><button class="btn btn-ghost btn-sm" type="submit">Balas</button></form>${(item.replies || []).map((reply) => `<div class="comment-reply"><span class="comment-avatar">${escapeHtml(initials(reply.author))}</span><div><strong>${escapeHtml(reply.author)}</strong><small>${escapeHtml(relativeTime(reply.createdAt))}</small><p>${escapeHtml(reply.text)}</p></div></div>`).join("")}</div></article>`).join("");
      const empty = $("#comments-empty");
      if (empty) empty.hidden = items.length > 0;
    }

    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      const account = currentAccount();
      if (!account) return toast("Masuk dulu untuk menulis komentar", "i-user");
      const input = $("#comment-input");
      const text = input.value.trim();
      if (!text) return;
      const comments = getComments();
      comments.push({ id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`, author: account.name, text, episodeTitle: panel.dataset.commentTitle, createdAt: Date.now(), likes: 0, replies: [] });
      saveComments(comments);
      input.value = "";
      render();
      toast("Komentar ditambahkan");
    });

    panel.addEventListener("click", (event) => {
      const sortButton = event.target.closest("[data-comment-sort]");
      if (sortButton) {
        sort = sortButton.dataset.commentSort;
        $$('[data-comment-sort]', panel).forEach((button) => button.classList.toggle("is-active", button === sortButton));
        render();
        return;
      }
      const replyToggle = event.target.closest("[data-comment-reply-toggle]");
      if (replyToggle) {
        if (!currentAccount()) return toast("Masuk dulu untuk membalas", "i-user");
        const replyForm = $(`[data-reply-form="${CSS.escape(replyToggle.dataset.commentReplyToggle)}"]`, panel);
        if (replyForm) {
          replyForm.hidden = !replyForm.hidden;
          if (!replyForm.hidden) $("input", replyForm).focus();
        }
        return;
      }
      const likeButton = event.target.closest("[data-comment-like]");
      if (likeButton) {
        if (!currentAccount()) return toast("Masuk dulu untuk menyukai komentar", "i-user");
        const commentId = likeButton.dataset.commentLike;
        const liked = readStore(`aniverse:comment-likes:${slug}`);
        const comments = getComments();
        const comment = comments.find((item) => item.id === commentId);
        if (!comment) return;
        if (liked.includes(commentId)) {
          comment.likes = Math.max(0, Number(comment.likes || 0) - 1);
          writeStore(`aniverse:comment-likes:${slug}`, liked.filter((id) => id !== commentId));
        } else {
          comment.likes = Number(comment.likes || 0) + 1;
          writeStore(`aniverse:comment-likes:${slug}`, [...liked, commentId]);
        }
        saveComments(comments);
        render();
      }
    });

    panel.addEventListener("submit", (event) => {
      const replyForm = event.target.closest("[data-reply-form]");
      if (!replyForm) return;
      event.preventDefault();
      const account = currentAccount();
      if (!account) return toast("Masuk dulu untuk membalas", "i-user");
      const input = $("input[name='reply']", replyForm);
      const text = input.value.trim();
      if (!text) return;
      const comments = getComments();
      const comment = comments.find((item) => item.id === replyForm.dataset.replyForm);
      if (!comment) return;
      comment.replies = comment.replies || [];
      comment.replies.push({ author: account.name, text, createdAt: Date.now() });
      saveComments(comments);
      render();
    });

    render();
  }

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
      }).finally(() => { offlineButton.disabled = false; });
      return;
    }

    const downloadButton = event.target.closest("[data-save-download]");
    if (downloadButton) {
      const downloads = readStore(STORE.downloads).filter((item) => item.url !== downloadButton.dataset.url);
      downloads.unshift({
        title: downloadButton.dataset.title,
        server: downloadButton.dataset.server,
        quality: downloadButton.dataset.quality,
        url: downloadButton.dataset.url,
        image: downloadButton.dataset.image,
        savedAt: Date.now(),
      });
      if (writeStore(STORE.downloads, downloads.slice(0, 100))) {
        toast("Tautan unduhan disimpan", "i-download");
        updateAccountStats();
      } else {
        toast("Penyimpanan perangkat penuh", "i-info");
      }
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

    const isClamped = () => text.scrollHeight > text.clientHeight + 4;
    if (!isClamped()) return;

    button.hidden = false;
    button.addEventListener("click", () => {
      const expanded = text.classList.toggle("is-expanded");
      button.textContent = expanded ? "Sembunyikan" : "Selengkapnya";
    });
  })();

  (function episodeTools() {
    const list = $("#episode-list");
    if (!list) return;

    const rows = $$(".episode-row", list);
    const input = $("#episode-filter");
    const empty = $("#episode-empty");
    const sortButton = $("#episode-sort");
    let order = sortButton?.dataset.order || "desc";

    const applyFilter = () => {
      const query = (input?.value || "").trim().toLowerCase();
      let visible = 0;
      rows.forEach((row) => {
        const haystack = (row.dataset.search || row.textContent).toLowerCase();
        const match = !query || haystack.includes(query);
        row.hidden = !match;
        if (match) visible += 1;
      });
      if (empty) empty.hidden = visible > 0;
    };

    const applyOrder = () => {
      const sorted = [...rows];
      if (order === "asc") sorted.reverse();
      sorted.forEach((row) => list.appendChild(row));
      rows.forEach((row) => row.classList.remove("is-first"));
      if (sorted[0]) sorted[0].classList.add("is-first");
    };

    if (input) {
      input.addEventListener("input", applyFilter);
    }

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
     Halaman tonton: server & riwayat
     --------------------------------------------------------- */

  (function player() {
    const frame = $("#video-player");
    const directLink = $("#player-open-current");
    const buttons = $$(".server-btn");

    if (buttons.length > 0) {
      buttons.forEach((button) => {
        button.addEventListener("click", () => {
          const src = button.dataset.src;
          if (!src) return;
          if (frame) {
            frame.src = src;
            frame.load?.();
          }
          if (directLink) directLink.href = src;
          buttons.forEach((other) => {
            other.classList.toggle("is-active", other === button);
            other.setAttribute("aria-pressed", other === button ? "true" : "false");
          });
          toast(`Server: ${button.textContent.trim()}`, "i-server");
        });
      });
    }

    const historyNode = $("#watch-history");
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
     Filter genre
     --------------------------------------------------------- */

  (function genreFilters() {
    const chips = $$("#genre-chips .chip");
    const counter = $("#genre-count");
    if (chips.length === 0) return;

    const update = () => {
      let count = 0;
      chips.forEach((chip) => {
        const input = $("input", chip);
        const checked = Boolean(input?.checked);
        chip.classList.toggle("is-selected", checked);
        if (checked) count += 1;
      });
      if (counter) counter.textContent = `${count} dipilih`;
    };

    chips.forEach((chip) => {
      const input = $("input", chip);
      if (!input) return;
      input.addEventListener("change", update);
    });
    update();
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
     Smart Anime Artwork Resolver (Client HD Enhancer)
     --------------------------------------------------------- */

  function initSmartArtworkResolver() {
    const images = $$("img.lazy-hd-cover[data-hd-title]");
    if (!images.length) return;

    const cacheKey = "otakunime:artworks";
    let memoryCache = {};
    try {
      const stored = sessionStorage.getItem(cacheKey);
      if (stored) memoryCache = JSON.parse(stored);
    } catch (_) {}

    const saveCache = (title, url) => {
      memoryCache[title.toLowerCase().trim()] = url;
      try {
        sessionStorage.setItem(cacheKey, JSON.stringify(memoryCache));
      } catch (_) {}
    };

    const upgradeImage = (img, hdUrl) => {
      if (!hdUrl || img.src === hdUrl) return;
      const preload = new Image();
      preload.src = hdUrl;
      preload.onload = () => {
        img.src = hdUrl;
        img.classList.add("is-hd-loaded");
      };
    };

    const fetchHdArtwork = async (img) => {
      const title = img.dataset.hdTitle;
      if (!title) return;
      const key = title.toLowerCase().trim();

      if (memoryCache[key]) {
        upgradeImage(img, memoryCache[key]);
        return;
      }

      try {
        const res = await fetch(`/api/artwork?title=${encodeURIComponent(title)}&fallback=${encodeURIComponent(img.src)}`);
        if (!res.ok) return;
        const data = await res.json();
        if (data && data.poster && data.score >= 45) {
          saveCache(title, data.poster);
          upgradeImage(img, data.poster);
        }
      } catch (_) {}
    };

    if ("IntersectionObserver" in window) {
      const observer = new IntersectionObserver(
        (entries, obs) => {
          entries.forEach((entry) => {
            if (entry.isIntersecting) {
              obs.unobserve(entry.target);
              fetchHdArtwork(entry.target);
            }
          });
        },
        { rootMargin: "300px 0px" }
      );
      images.forEach((img) => observer.observe(img));
    } else {
      images.forEach(fetchHdArtwork);
    }
  }

  /* ---------------------------------------------------------
     Init
     --------------------------------------------------------- */

  applyImageFallbacks();
  paintBookmarkButtons();
  renderContinueWatching();
  renderBookmarkPage();
  initAccountPage();
  initHistoryPage();
  initDownloadCollection();
  initOfflineCollection();
  initComments();
  initSmartArtworkResolver();
})();
