// Rample Manager website: step-grid animation, download links and manual navigation.
(() => {
  const REPO = "OliverLSanz/rample-manager";
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // A 4×4 step grid like the app icon, playing a small pattern.
  const pads = document.querySelector("[data-pads]");
  if (pads) {
    const colors = ["var(--v1)", "var(--v2)", "var(--v3)", "var(--v4)"];
    const pattern = [
      [1, 0, 1, 0],
      [0, 1, 0, 1],
      [1, 1, 1, 1],
      [0, 0, 1, 0],
    ];
    const cells = [];
    pattern.forEach((row, r) =>
      row.forEach((on, c) => {
        const pad = document.createElement("div");
        pad.className = "pad" + (on ? " on" : "");
        pad.style.setProperty("--c", colors[r]);
        pads.append(pad);
        cells.push({ pad, r, c });
      }),
    );
    if (!reduced) {
      let step = 0;
      setInterval(() => {
        cells.forEach(({ pad, c }) => pad.classList.toggle("hit", c === step && pad.classList.contains("on")));
        step = (step + 1) % 4;
      }, 260);
      // Occasionally vary the pattern so the grid feels alive.
      setInterval(() => {
        const { pad } = cells[Math.floor(Math.random() * cells.length)];
        pad.classList.toggle("on");
      }, 1800);
    }
  }

  // Highlight the download for this computer.
  const platform = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent).toLowerCase();
  const os = platform.includes("win") ? "windows" : platform.includes("mac") ? "mac" : null;
  if (os) {
    const card = document.querySelector(`.platform[data-os="${os}"]`);
    card?.classList.add("recommended");
    const primary = document.querySelector("[data-primary-download]");
    const link = card?.querySelector("a.button");
    if (primary && link) {
      primary.href = link.href;
      primary.querySelector("[data-primary-label]").textContent =
        os === "mac" ? "Download for macOS" : "Download for Windows";
    }
  }

  // Show the current version and installer sizes from the latest release.
  const versions = document.querySelectorAll("[data-version]");
  const sizes = document.querySelectorAll("[data-size]");
  if (versions.length || sizes.length) {
    fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((release) => {
        const version = release.tag_name.replace(/^v/, "");
        const date = new Date(release.published_at).toLocaleDateString("en", {
          year: "numeric",
          month: "short",
          day: "numeric",
        });
        versions.forEach((el) => (el.textContent = `Version ${version} · ${date}`));
        sizes.forEach((el) => {
          const asset = release.assets.find((a) => a.name === el.dataset.size);
          if (asset) el.textContent = `· ${(asset.size / 1048576).toFixed(0)} MB`;
        });
      })
      .catch(() => {});
  }

  // Video tour: the 9:16 cut on phones held upright, a play button over the poster.
  const tour = document.querySelector("[data-tour]");
  if (tour) {
    const video = tour.querySelector("video");
    const play = tour.querySelector(".tour-play");
    if (matchMedia("(max-width: 640px) and (orientation: portrait)").matches) {
      tour.classList.add("vertical");
      video.width = 1080;
      video.height = 1920;
      video.poster = "assets/video/tour-poster-vertical.webp";
      video.querySelector("source").src = "assets/video/tour-vertical.mp4";
      video.load();
    }
    video.controls = false;
    play.hidden = false;
    play.addEventListener("click", () => {
      play.hidden = true;
      video.controls = true;
      video.play().catch(() => {
        play.hidden = false;
      });
    });
  }

  // Reveal sections as they scroll into view.
  const reveals = document.querySelectorAll(".reveal");
  if ("IntersectionObserver" in window && !reduced) {
    const io = new IntersectionObserver(
      (entries) =>
        entries.forEach((e) => {
          if (e.isIntersecting) {
            e.target.classList.add("in");
            io.unobserve(e.target);
          }
        }),
      { rootMargin: "0px 0px -8% 0px" },
    );
    reveals.forEach((el) => io.observe(el));
  } else {
    reveals.forEach((el) => el.classList.add("in"));
  }

  // Manual: mark the chapter being read in the table of contents.
  const toc = document.querySelectorAll(".toc a");
  if (toc.length && "IntersectionObserver" in window) {
    const byId = new Map([...toc].map((a) => [a.hash.slice(1), a]));
    const spy = new IntersectionObserver(
      (entries) =>
        entries.forEach((e) => {
          if (!e.isIntersecting) return;
          toc.forEach((a) => a.classList.remove("active"));
          byId.get(e.target.id)?.classList.add("active");
        }),
      { rootMargin: "-20% 0px -70% 0px" },
    );
    document.querySelectorAll(".chapter[id]").forEach((c) => spy.observe(c));
  }
})();
