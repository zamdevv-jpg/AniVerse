(() => {
  const body = document.body;
  const supabaseUrl = body.dataset.supabaseUrl || "";
  const anonKey = body.dataset.supabaseAnonKey || "";
  const configured = Boolean(supabaseUrl && anonKey);
  const accountStatus = document.querySelector("#google-auth-status");
  const loginButtons = [
    document.querySelector("#google-login"),
    document.querySelector("#uploads-google-login"),
  ].filter(Boolean);
  const logoutButton = document.querySelector("#google-logout");
  const accountAuthCard = document.querySelector("#supabase-auth-card");
  const configNotice = document.querySelector("#supabase-config-missing");
  const loginNotice = document.querySelector("#upload-auth-required");
  const adminNotice = document.querySelector("#upload-admin-denied");
  const adminPanel = document.querySelector("#upload-admin-panel");
  const listNode = document.querySelector("#uploaded-anime-list");
  const listStatus = document.querySelector("#upload-list-status");
  const uploadForm = document.querySelector("#upload-anime-form");
  const progressNode = document.querySelector("#upload-progress");
  const countNode = document.querySelector("#upload-count");
  const playerPanel = document.querySelector("#uploaded-player");
  const videoNode = document.querySelector("#uploaded-video");
  const clientFactory = window.supabase?.createClient;

  if (configNotice) configNotice.hidden = configured;
  if (loginNotice) loginNotice.hidden = !configured;
  if (!configured) {
    if (accountStatus) accountStatus.textContent = "Login Google belum aktif karena konfigurasi Supabase belum diisi.";
    loginButtons.forEach((button) => (button.disabled = true));
    if (listStatus) listStatus.textContent = "Koleksi cloud belum dikonfigurasi.";
    return;
  }

  if (!clientFactory) {
    if (accountStatus) accountStatus.textContent = "Library Supabase belum termuat. Periksa koneksi internet lalu muat ulang.";
    if (listStatus) listStatus.textContent = "Koleksi cloud tidak dapat dimuat.";
    loginButtons.forEach((button) => (button.disabled = true));
    return;
  }

  const client = clientFactory(supabaseUrl, anonKey, {
    auth: { persistSession: true, autoRefreshToken: true, detectSessionInUrl: true },
  });
  window.aniVerseSupabase = client;
  window.aniVerseAuth = {
    client,
    user: null,
    getUser() {
      return this.user;
    },
    async updateProfile(displayName, avatarFile = null) {
      const name = String(displayName || "").trim();
      if (!name || name.length > 40) throw new Error("Nama profil harus 1–40 karakter.");
      const user = this.user;
      if (!user) throw new Error("Masuk dengan Google untuk mengubah profil.");
      let avatarUrl = user.user_metadata?.avatar_url || user.user_metadata?.picture || null;
      let uploadedPath = "";
      if (avatarFile) {
        const extensions = { "image/jpeg": "jpg", "image/png": "png", "image/webp": "webp" };
        const extension = extensions[avatarFile.type];
        if (!extension) throw new Error("Foto harus berformat JPG, PNG, atau WebP.");
        if (avatarFile.size > 2 * 1024 * 1024) throw new Error("Ukuran foto maksimal 2 MB.");
        const fileId = globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(36).slice(2)}`;
        uploadedPath = `${user.id}/${fileId}.${extension}`;
        const { error: uploadError } = await client.storage.from("aniverse-avatars").upload(uploadedPath, avatarFile, {
          cacheControl: "3600",
          contentType: avatarFile.type,
          upsert: false,
        });
        if (uploadError) throw uploadError;
        avatarUrl = client.storage.from("aniverse-avatars").getPublicUrl(uploadedPath).data.publicUrl;
      }
      const { data, error } = await client.auth.updateUser({
        data: { full_name: name, name, ...(avatarUrl ? { avatar_url: avatarUrl } : {}) },
      });
      if (error) {
        if (uploadedPath) await client.storage.from("aniverse-avatars").remove([uploadedPath]);
        throw error;
      }
      setAuthView({ user: data.user });
      return data.user;
    },
  };

  const escapeHtml = (value) =>
    String(value ?? "").replace(/[&<>"']/g, (char) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    })[char]);

  const isAdmin = (user) => user?.app_metadata?.role === "admin";

  const setAuthView = (session) => {
    const user = session?.user;
    window.aniVerseAuth.user = user || null;
    window.aniVerseAuthUser = user || null;
    if (accountStatus) {
      accountStatus.textContent = user
        ? `Masuk sebagai ${user.email || user.user_metadata?.full_name || "akun Google"}.`
        : "Masuk dengan Google untuk menghubungkan akun Supabase.";
    }
    loginButtons.forEach((button) => (button.hidden = Boolean(user)));
    if (accountAuthCard) accountAuthCard.hidden = Boolean(user);
    if (logoutButton) logoutButton.hidden = !user;
    if (loginNotice) loginNotice.hidden = Boolean(user);
    if (adminNotice) adminNotice.hidden = !user || isAdmin(user);
    if (adminPanel) adminPanel.hidden = !isAdmin(user);
    window.dispatchEvent(new CustomEvent("aniverse:auth-change", { detail: { user: user || null } }));
  };

  const signIn = async (button) => {
    button.disabled = true;
    try {
      const { error } = await client.auth.signInWithOAuth({
        provider: "google",
        options: { redirectTo: `${window.location.origin}${window.location.pathname}` },
      });
      if (error) throw error;
    } catch (error) {
      if (accountStatus) accountStatus.textContent = `Login Google gagal: ${error.message}`;
      if (progressNode) progressNode.textContent = `Login Google gagal: ${error.message}`;
      button.disabled = false;
    }
  };

  loginButtons.forEach((button) => button.addEventListener("click", () => signIn(button)));
  logoutButton?.addEventListener("click", async () => {
    const { error } = await client.auth.signOut();
    if (error && accountStatus) accountStatus.textContent = `Gagal keluar: ${error.message}`;
  });

  const publicUrl = (path) =>
    client.storage.from("aniverse-media").getPublicUrl(path).data.publicUrl;

  const renderUploads = (rows) => {
    if (!listNode) return;
    if (countNode) countNode.textContent = `${rows.length} episode`;
    if (!rows.length) {
      listNode.innerHTML = "";
      if (listStatus) listStatus.textContent = "Belum ada anime yang diunggah.";
      return;
    }
    if (listStatus) listStatus.textContent = "";
    listNode.innerHTML = rows
      .map((row) => {
        const poster = row.poster_path
          ? `<img src="${escapeHtml(publicUrl(row.poster_path))}" alt="" loading="lazy">`
          : `<span class="uploaded-poster-fallback">${escapeHtml(row.title.slice(0, 1).toUpperCase())}</span>`;
        return `<article class="uploaded-card">
          <div class="uploaded-poster">${poster}</div>
          <div class="uploaded-card-copy">
            <strong>${escapeHtml(row.title)}</strong>
            <span>Episode ${escapeHtml(row.episode_number)}</span>
            <button class="btn btn-gold btn-sm" type="button" data-uploaded-path="${escapeHtml(row.video_path)}" data-uploaded-title="${escapeHtml(row.title)}" data-uploaded-episode="${escapeHtml(row.episode_number)}" data-uploaded-synopsis="${escapeHtml(row.synopsis)}">Tonton episode</button>
          </div>
        </article>`;
      })
      .join("");
  };

  const loadUploads = async () => {
    if (!listNode) return;
    if (listStatus) listStatus.textContent = "Memuat koleksi cloud…";
    const { data, error } = await client
      .from("uploaded_episodes")
      .select("id,title,slug,episode_number,synopsis,poster_path,video_path,created_at")
      .order("created_at", { ascending: false })
      .limit(100);
    if (error) {
      if (listStatus) listStatus.textContent = `Koleksi cloud gagal dimuat: ${error.message}`;
      return;
    }
    renderUploads(data || []);
  };

  listNode?.addEventListener("click", (event) => {
    const button = event.target.closest("[data-uploaded-path]");
    if (!button || !videoNode || !playerPanel) return;
    videoNode.src = publicUrl(button.dataset.uploadedPath);
    document.querySelector("#uploaded-player-title").textContent =
      `${button.dataset.uploadedTitle} · Episode ${button.dataset.uploadedEpisode}`;
    document.querySelector("#uploaded-player-synopsis").textContent = button.dataset.uploadedSynopsis || "";
    playerPanel.hidden = false;
    playerPanel.scrollIntoView({ behavior: "smooth", block: "start" });
    videoNode.play().catch(() => {});
  });

  document.querySelector("#close-uploaded-player")?.addEventListener("click", () => {
    if (!videoNode || !playerPanel) return;
    videoNode.pause();
    videoNode.removeAttribute("src");
    videoNode.load();
    playerPanel.hidden = true;
  });

  const slugify = (title) =>
    title
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || crypto.randomUUID();

  const fileExtension = (file, fallback) => {
    const ext = file.name.split(".").pop()?.toLowerCase();
    return /^[a-z0-9]{2,5}$/.test(ext || "") ? ext : fallback;
  };

  const uploadLargeVideo = (file, path, session, onProgress) =>
    new Promise((resolve, reject) => {
      if (!window.tus?.Upload) {
        reject(new Error("Tus upload belum termuat. Muat ulang halaman dan coba lagi."));
        return;
      }
      const project = new URL(supabaseUrl);
      const projectRef = project.hostname.split(".")[0];
      const storageOrigin = project.hostname.endsWith(".supabase.co")
        ? `${project.protocol}//${projectRef}.storage.supabase.co`
        : project.origin;
      const upload = new window.tus.Upload(file, {
        endpoint: `${storageOrigin}/storage/v1/upload/resumable`,
        retryDelays: [0, 3000, 5000, 10000, 20000],
        headers: {
          authorization: `Bearer ${session.access_token}`,
          apikey: anonKey,
          "x-upsert": "false",
        },
        uploadDataDuringCreation: true,
        removeFingerprintOnSuccess: true,
        chunkSize: 6 * 1024 * 1024,
        metadata: {
          bucketName: "aniverse-media",
          objectName: path,
          contentType: file.type || "application/octet-stream",
          cacheControl: "3600",
        },
        onError: reject,
        onProgress: (uploaded, total) => onProgress(Math.round((uploaded / total) * 100)),
        onSuccess: resolve,
      });
      upload.findPreviousUploads().then((previous) => {
        if (previous.length) upload.resumeFromPreviousUpload(previous[0]);
        upload.start();
      }).catch(reject);
    });

  uploadForm?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const submit = document.querySelector("#upload-submit");
    submit.disabled = true;
    try {
      const { data: sessionData, error: sessionError } = await client.auth.getSession();
      if (sessionError) throw sessionError;
      const session = sessionData.session;
      if (!isAdmin(session?.user)) throw new Error("Akun Google ini belum memiliki role admin.");

      const formData = new FormData(uploadForm);
      const title = String(formData.get("title") || "").trim();
      const episodeNumber = String(formData.get("episode_number") || "").trim();
      const synopsis = String(formData.get("synopsis") || "").trim();
      const videoFile = formData.get("video");
      const posterFile = formData.get("poster");
      if (!(videoFile instanceof File) || videoFile.size === 0) throw new Error("Pilih berkas video episode.");
      if (videoFile.size > 5 * 1024 * 1024 * 1024) throw new Error("Ukuran video melebihi batas 5 GB.");
      if (!videoFile.type.startsWith("video/")) throw new Error("Format berkas harus berupa video.");

      const slug = slugify(title);
      const mediaId = crypto.randomUUID();
      const videoPath = `anime/${session.user.id}/${slug}/${mediaId}-episode.${fileExtension(videoFile, "mp4")}`;
      let posterPath = null;

      if (posterFile instanceof File && posterFile.size > 0) {
        posterPath = `anime/${session.user.id}/${slug}/${mediaId}-poster.${fileExtension(posterFile, "jpg")}`;
        const { error } = await client.storage.from("aniverse-media").upload(posterPath, posterFile, {
          cacheControl: "3600",
          upsert: false,
          contentType: posterFile.type,
        });
        if (error) throw error;
      }

      if (progressNode) progressNode.textContent = "Mengunggah video: 0%";
      await uploadLargeVideo(videoFile, videoPath, session, (percent) => {
        if (progressNode) progressNode.textContent = `Mengunggah video: ${percent}%`;
      });

      const { error: insertError } = await client.from("uploaded_episodes").insert({
        title,
        slug,
        episode_number: episodeNumber,
        synopsis,
        poster_path: posterPath,
        video_path: videoPath,
        created_by: session.user.id,
      });
      if (insertError) {
        await client.storage.from("aniverse-media").remove([videoPath, ...(posterPath ? [posterPath] : [])]);
        throw insertError;
      }

      uploadForm.reset();
      if (progressNode) progressNode.textContent = "Episode berhasil diunggah.";
      await loadUploads();
    } catch (error) {
      if (progressNode) progressNode.textContent = `Unggah gagal: ${error.message}`;
    } finally {
      submit.disabled = false;
    }
  });

  const refresh = async () => {
    const { data, error } = await client.auth.getSession();
    if (error && accountStatus) accountStatus.textContent = `Sesi Supabase gagal dibaca: ${error.message}`;
    const session = data?.session || null;
    setAuthView(session);
    if (listNode) await loadUploads();
  };

  client.auth.onAuthStateChange((_event, session) => {
    setAuthView(session);
    if (listNode) window.setTimeout(loadUploads, 0);
  });

  refresh();
})();
