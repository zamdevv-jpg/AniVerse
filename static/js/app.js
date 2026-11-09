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

  function relativeTime(timestamp) {
    const minutes = Math.max(1, Math.floor((Date.now() - Number(timestamp || Date.now())) / 60000));
    if (minutes < 60) return `${minutes} menit lalu`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} jam lalu`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days} hari lalu`;
    return new Date(Number(timestamp)).toLocaleDateString("id-ID", { day: "numeric", month: "short" });
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
      const allowedTypes = ["image/jpeg", "image/png", "image/webp"];
      if (!allowedTypes.includes(file.type)) {
        avatarFile.value = "";
        return toast("Pilih foto JPG, PNG, atau WebP", "i-info");
      }
      if (file.size > 2 * 1024 * 1024) {
        avatarFile.value = "";
        return toast("Ukuran foto maksimal 2 MB", "i-info");
      }
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      selectedAvatar = file;
      previewUrl = URL.createObjectURL(file);
      if (photoName) photoName.textContent = file.name;
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
            <small>Ditambahkan ${escapeHtml(relativeTime(item.savedAt))}</small>
          </div>
          <a class="btn btn-gold btn-sm" href="${escapeHtml(item.url)}" target="_blank" rel="noopener noreferrer nofollow">Buka</a>
          <button class="icon-btn remove-download" type="button" data-remove-download="${index}" aria-label="Hapus tautan"><svg class="icon"><use href="#i-x"/></svg></button>
        </article>`,
        )
        .join("");
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
          <form class="reply-form" data-reply-form="${escapeHtml(item.id)}" hidden>
            <input name="reply" maxlength="300" placeholder="Tulis balasan..." required>
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
      toast(parentId ? "Balasan dikirim" : "Komentar ditambahkan");
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
    const qualityToggle = $("#player-quality-toggle");
    const qualityMenu = $("#player-quality-menu");
    const qualityLabel = $("#player-quality-label");
    const qualityButtons = $$("#player-quality-menu [data-quality]");
    const status = $("#player-status");
    const bigPlay = $("#player-play");

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

    const setStatus = (text, error = false, quiet = false) => {
      if (!status) return;
      status.textContent = text;
      status.classList.toggle("is-error", Boolean(error));
      status.classList.toggle("is-quiet", Boolean(quiet));
    };

    const paintQuality = () => {
      qualityButtons.forEach((button) => {
        const value = button.dataset.quality;
        const active = value === "auto" ? mode === "auto" : mode === value;
        button.classList.toggle("is-active", active);
        button.classList.toggle("is-selected", active);
        button.setAttribute("aria-checked", active ? "true" : "false");
      });
      const text =
        mode === "auto"
          ? group
            ? `Otomatis · ${group.quality}`
            : "Otomatis"
          : group
            ? group.quality
            : mode;
      if (qualityLabel) qualityLabel.textContent = text;
      if (qualityToggle) qualityToggle.setAttribute("aria-label", `Resolusi ${text}`);
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
      if (!group) group = mode === "auto" ? pickAutoGroup() : groups.find((entry) => entry.quality === mode) || pickAutoGroup();
      if (!group) {
        setStatus("Tidak ada sumber yang bisa diputar", true);
        return;
      }
      if (bigPlay) bigPlay.classList.add("is-hidden");
      if (!video.src) {
        loadCurrent(true);
      } else {
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
      setStatus(`Siap diputar · ${mode === "auto" ? "Otomatis" : group.quality}`);
    }

    bigPlay?.addEventListener("click", startPlayback);
    video.addEventListener("click", () => {
      if (!started) startPlayback();
    });

    const closeQualityMenu = (restoreFocus = false) => {
      if (!qualityMenu || !qualityToggle) return;
      qualityMenu.hidden = true;
      qualityToggle.setAttribute("aria-expanded", "false");
      qualityControl?.classList.remove("is-open");
      if (restoreFocus) qualityToggle.focus();
    };

    const openQualityMenu = () => {
      if (!qualityMenu || !qualityToggle) return;
      qualityMenu.hidden = false;
      qualityToggle.setAttribute("aria-expanded", "true");
      qualityControl?.classList.add("is-open");
    };

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

    document.addEventListener("click", (event) => {
      if (qualityControl && !qualityControl.contains(event.target)) closeQualityMenu();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && qualityMenu && !qualityMenu.hidden) {
        closeQualityMenu(true);
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
      if (historyNode) {
        const next = $(".episode-nav a[href*='/watch/']:last-child");
        if (next && /berikutnya/i.test(next.textContent)) {
          toast("Episode selesai. Episode berikutnya tersedia.", "i-check");
        }
      }
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
