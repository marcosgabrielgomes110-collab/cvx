(() => {
  "use strict";

  const root = document.documentElement;
  const body = document.body;
  const live = document.getElementById("live-region");
  const themeButton = document.getElementById("theme-toggle");
  const menuButton = document.getElementById("menu-toggle");
  const sidebar = document.getElementById("sidebar");
  const progress = document.getElementById("progress");
  const searchOverlay = document.getElementById("search-overlay");
  const searchInput = document.getElementById("search-input");
  const searchResults = document.getElementById("search-results");
  const searchEmpty = document.getElementById("search-empty");
  const searchTrigger = document.querySelector("[data-search-open]");

  const announce = (message) => {
    if (!live) return;
    live.textContent = "";
    window.setTimeout(() => { live.textContent = message; }, 20);
  };

  /* Theme ---------------------------------------------------------------- */
  const storedTheme = localStorage.getItem("cvx-theme");
  const queryTheme = new URLSearchParams(window.location.search).get("theme");
  const initialTheme = queryTheme === "light" || queryTheme === "dark"
    ? queryTheme
    : storedTheme === "light" || storedTheme === "dark" ? storedTheme : "dark";

  const applyTheme = (theme) => {
    root.dataset.theme = theme;
    localStorage.setItem("cvx-theme", theme);
    if (themeButton) {
      const next = theme === "dark" ? "light" : "dark";
      themeButton.setAttribute("aria-label", `Switch to ${next} theme`);
      themeButton.setAttribute("title", `Switch to ${next} theme`);
    }
    const meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.setAttribute("content", theme === "dark" ? "#08090a" : "#f5f5f2");
  };

  applyTheme(initialTheme);

  themeButton?.addEventListener("click", () => {
    applyTheme(root.dataset.theme === "dark" ? "light" : "dark");
  });

  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", (event) => {
    if (!localStorage.getItem("cvx-theme")) applyTheme(event.matches ? "light" : "dark");
  });

  /* Mobile navigation ---------------------------------------------------- */
  const setMenu = (open) => {
    body.classList.toggle("nav-open", open);
    menuButton?.setAttribute("aria-expanded", String(open));
    menuButton?.setAttribute("aria-label", open ? "Close navigation" : "Open navigation");
    if (open) sidebar?.querySelector("a")?.focus({ preventScroll: true });
  };

  menuButton?.addEventListener("click", () => setMenu(!body.classList.contains("nav-open")));
  sidebar?.querySelectorAll("a").forEach((link) => {
    link.addEventListener("click", () => setMenu(false));
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && body.classList.contains("nav-open")) {
      setMenu(false);
      menuButton?.focus();
    }
  });

  document.addEventListener("click", (event) => {
    if (!body.classList.contains("nav-open")) return;
    if (sidebar?.contains(event.target) || menuButton?.contains(event.target)) return;
    setMenu(false);
  });

  /* Copy buttons --------------------------------------------------------- */
  const fallbackCopy = (text) => {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const copied = document.execCommand("copy");
    area.remove();
    return copied;
  };

  document.querySelectorAll("[data-copy-target]").forEach((button) => {
    button.addEventListener("click", async () => {
      const target = document.getElementById(button.dataset.copyTarget);
      if (!target) return;
      const text = target.textContent.trim();
      let copied = false;
      try {
        await navigator.clipboard.writeText(text);
        copied = true;
      } catch {
        copied = fallbackCopy(text);
      }
      if (!copied) {
        announce("Copy failed. Select the text and copy manually.");
        return;
      }
      const original = button.textContent;
      button.textContent = "Copied";
      button.classList.add("is-copied");
      announce("Code copied to clipboard.");
      window.setTimeout(() => {
        button.textContent = original;
        button.classList.remove("is-copied");
      }, 1600);
    });
  });

  /* Tabs ----------------------------------------------------------------- */
  document.querySelectorAll("[data-tabs]").forEach((tabs) => {
    const buttons = [...tabs.querySelectorAll('[role="tab"]')];
    const panels = [...tabs.querySelectorAll('[role="tabpanel"]')];

    const select = (index, focus = false) => {
      buttons.forEach((button, position) => {
        const selected = position === index;
        button.classList.toggle("is-active", selected);
        button.setAttribute("aria-selected", String(selected));
        button.tabIndex = selected ? 0 : -1;
        panels[position]?.toggleAttribute("hidden", !selected);
        panels[position]?.classList.toggle("is-active", selected);
      });
      if (focus) buttons[index]?.focus();
    };

    buttons.forEach((button, index) => {
      button.addEventListener("click", () => select(index));
      button.addEventListener("keydown", (event) => {
        if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const next = event.key === "Home" ? 0
          : event.key === "End" ? buttons.length - 1
          : (index + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length;
        select(next, true);
      });
    });
  });

  /* Search --------------------------------------------------------------- */
  const searchItems = [];
  const seenAnchors = new Set();

  const pushItem = (anchor, title, context, text) => {
    if (!anchor || seenAnchors.has(anchor)) return;
    seenAnchors.add(anchor);
    searchItems.push({
      anchor,
      title,
      context,
      haystack: `${title} ${context} ${text}`.toLowerCase().replace(/\s+/g, " "),
    });
  };

  document.querySelectorAll("article section[id]").forEach((section) => {
    const heading = section.querySelector("h2");
    if (!heading) return;
    const title = heading.textContent.trim();
    const bodyText = section.textContent.trim().replace(/\s+/g, " ");
    pushItem(section.id, title, "Section", bodyText);
    section.querySelectorAll("h3").forEach((subheading) => {
      const id = subheading.id || slug(subheading.textContent);
      if (id && !subheading.id) subheading.id = id;
      pushItem(id, subheading.textContent.trim(), title, subheading.parentElement?.textContent || "");
    });
  });

  document.querySelectorAll("#commands tbody tr").forEach((row) => {
    const command = row.querySelector("td")?.textContent.trim();
    if (!command) return;
    const text = row.textContent.trim().replace(/\s+/g, " ");
    searchItems.push({
      anchor: "commands",
      title: command,
      context: "Command",
      haystack: `${command} ${text}`.toLowerCase(),
    });
  });

  const openSearch = () => {
    if (!searchOverlay || !searchInput) return;
    searchOverlay.hidden = false;
    body.classList.add("search-open");
    searchInput.value = "";
    renderSearch("");
    window.setTimeout(() => searchInput.focus(), 10);
  };

  const closeSearch = () => {
    if (!searchOverlay) return;
    searchOverlay.hidden = true;
    body.classList.remove("search-open");
    searchTrigger?.focus({ preventScroll: true });
  };

  const renderSearch = (query) => {
    if (!searchResults) return;
    const terms = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
    const matches = terms.length
      ? searchItems.filter((item) => terms.every((term) => item.haystack.includes(term))).slice(0, 12)
      : searchItems.filter((item) => item.context === "Section").slice(0, 8);

    searchResults.innerHTML = "";
    matches.forEach((item, index) => {
      const link = document.createElement("a");
      link.className = "search-result";
      link.href = `#${item.anchor}`;
      link.dataset.index = String(index);
      link.innerHTML = `<span>${escapeHtml(item.title)}</span><small>${escapeHtml(item.context)}</small>`;
      link.addEventListener("click", (event) => {
        event.preventDefault();
        closeSearch();
        document.getElementById(item.anchor)?.scrollIntoView({ behavior: "smooth", block: "start" });
      });
      searchResults.appendChild(link);
    });
    if (matches.length) {
      searchResults.firstElementChild?.classList.add("is-selected");
      searchEmpty.hidden = true;
    } else {
      searchEmpty.hidden = false;
    }
  };

  searchTrigger?.addEventListener("click", openSearch);
  searchOverlay?.addEventListener("click", (event) => {
    if (event.target === searchOverlay) closeSearch();
  });
  searchInput?.addEventListener("input", (event) => renderSearch(event.target.value));
  searchOverlay?.addEventListener("keydown", (event) => {
    if (event.key !== "Tab") return;
    const focusable = searchOverlay.querySelectorAll('a[href], button:not([disabled]), input:not([disabled])');
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  });

  searchInput?.addEventListener("keydown", (event) => {
    const results = [...searchResults.querySelectorAll(".search-result")];
    if (!results.length) return;
    const current = results.findIndex((item) => item.classList.contains("is-selected"));
    const move = (delta) => {
      const next = (current + delta + results.length) % results.length;
      results.forEach((item, index) => item.classList.toggle("is-selected", index === next));
      results[next].scrollIntoView({ block: "nearest" });
    };
    if (event.key === "ArrowDown") { event.preventDefault(); move(1); }
    if (event.key === "ArrowUp") { event.preventDefault(); move(-1); }
    if (event.key === "Enter") { event.preventDefault(); results[Math.max(current, 0)]?.click(); }
  });

  document.addEventListener("keydown", (event) => {
    const isTyping = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName || "");
    const searchShortcut = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k";
    const slashShortcut = event.key === "/" && !isTyping;
    if (searchShortcut || slashShortcut) {
      event.preventDefault();
      openSearch();
    }
    if (event.key === "Escape" && !searchOverlay.hidden) closeSearch();
  });

  const initialQuery = new URLSearchParams(window.location.search).get("q");
  if (initialQuery) {
    openSearch();
    searchInput.value = initialQuery;
    renderSearch(initialQuery);
  }

  /* Active navigation and reading progress ------------------------------- */
  const navLinks = [...document.querySelectorAll('.sidebar a[href^="#"], .toc a[href^="#"]')];
  const sections = [...document.querySelectorAll("article section[id]")];

  const setActive = (id) => {
    navLinks.forEach((link) => {
      link.classList.toggle("is-active", link.getAttribute("href") === `#${id}`);
    });
  };

  if ("IntersectionObserver" in window) {
    const observer = new IntersectionObserver((entries) => {
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((a, b) => b.intersectionRatio - a.intersectionRatio);
      if (visible[0]) setActive(visible[0].target.id);
    }, { rootMargin: "-18% 0px -68% 0px", threshold: [0.05, 0.2, 0.5] });
    sections.forEach((section) => observer.observe(section));
  }

  const updateProgress = () => {
    if (!progress) return;
    const scrollable = document.documentElement.scrollHeight - window.innerHeight;
    const ratio = scrollable > 0 ? Math.min(1, window.scrollY / scrollable) : 0;
    progress.style.width = `${ratio * 100}%`;
  };

  window.addEventListener("scroll", updateProgress, { passive: true });
  window.addEventListener("resize", updateProgress);
  updateProgress();

  /* Helpers -------------------------------------------------------------- */
  function slug(text) {
    return text.toLowerCase().trim()
      .replace(/[^a-z0-9\s-]/g, "")
      .replace(/\s+/g, "-")
      .replace(/-+/g, "-");
  }

  function escapeHtml(text) {
    return text.replace(/[&<>"']/g, (character) => ({
      "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#039;",
    }[character]));
  }
})();
