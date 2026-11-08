/* =========================================================
   AniVerse — interaksi klien
   Semua data pribadi (bookmark & riwayat) disimpan di localStorage.
   ========================================================= */
(() => {
  "use strict";

  const STORE = {
    bookmarks: "otakunime:bookmarks",
    history: "otakunime:history",
  };

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
  initSmartArtworkResolver();
})();
