/* ============================================================
   CrateBay — Official Website Scripts
   ============================================================ */

(function () {
  'use strict';

  // ----------------------------------------------------------
  // i18n — Bilingual content (English / Chinese)
  // ----------------------------------------------------------
  const i18n = {
    en: {
      // Navbar
      navFeatures: 'Features',
      navDemo: 'Demo',
      navTech: 'Tech Stack',
      navCompare: 'Compare',
      navInstall: 'Install',

      // Theme toggle
      themeToggle: 'Toggle dark/light mode',

      // Hero
      heroBadge: 'v1.0.0 Stable Release',
      heroTitle1: 'Crate',
      heroTitle2: 'Bay',
      heroTagline:
        'The open-source desktop app for managing Docker containers, Linux VMs, and Kubernetes \u2014 blazing fast, built with Rust.',
      heroDownload: 'Download for macOS',
      heroGithub: 'View on GitHub',
      statLang: 'Pure Rust',
      statLicense: 'Open Source',
      statSize: 'App Size',

      // Features
      featLabel: 'Features',
      featTitle: 'Everything you need to manage containers and beyond.',
      featDesc:
        'CrateBay brings a unified, native-speed experience for Docker containers, Linux VMs, and Kubernetes clusters \u2014 without the overhead. Stay productive with system tray quick actions and live running counts.',
      feat1Title: 'Docker Container Management',
      feat1Desc:
        'Full lifecycle control \u2014 create, start, stop, restart, remove. Real-time log streaming and exec into running containers. Port mappings remain consistently ordered during live refresh.',
      feat2Title: 'Linux Virtual Machines',
      feat2Desc:
        'Native VM support via Virtualization.framework (macOS), KVM (Linux), and Hyper-V (Windows). Boot in seconds.',
      feat3Title: 'Image & Volume Management',
      feat3Desc:
        'Search and pull images from Docker Hub and private registries. Manage volumes with inspect, create, prune, and mount operations. Volume lists remain consistently ordered during live refresh.',
      feat4Title: 'Resource Monitoring',
      feat4Desc:
        'Real-time CPU, memory, network, and disk I/O monitoring for every container and VM. Spot issues instantly.',
      feat5Title: 'Port Forwarding & VirtioFS',
      feat5Desc:
        'Automatic port forwarding and near-native file sharing between host and guest via VirtioFS.',
      feat6Title: 'Kubernetes (K3s)',
      feat6Desc:
        'Built-in K3s integration with pod, service, and deployment dashboards. Manage namespaces and workloads effortlessly.',
      feat7Title: 'Plugin System',
      feat7Desc:
        'Extensible architecture with lifecycle hooks. Build custom plugins to integrate your own tools and workflows.',
      feat8Title: 'Cross-platform',
      feat8Desc:
        'Runs on macOS, Linux, and Windows. Same interface, same speed, same experience on every platform.',

      // Demo
      demoLabel: 'Interface',
      demoTitle: 'Built for speed and clarity.',
      demoDesc:
        'A clean, responsive GUI built with Tauri and React. Every interaction feels instant.',
      demoTitlebar: 'CrateBay',
      demoSidebar0: 'Dashboard',
      demoSidebar1: 'Containers',
      demoSidebar2: 'Images',
      demoSidebar3: 'Volumes',
      demoSidebar4: 'VMs',
      demoSidebar5: 'Kubernetes',
      demoHeader: 'Containers',
      demoBtn: '+ New',
      demoThName: 'Name',
      demoThImage: 'Image',
      demoThStatus: 'Status',
      demoThCpu: 'CPU',
      demoThMem: 'Memory',
      demoR1Name: 'web-frontend',
      demoR1Img: 'nginx:alpine',
      demoR1Status: 'Running',
      demoR1Cpu: '0.3%',
      demoR1Mem: '24 MB',
      demoR2Name: 'api-server',
      demoR2Img: 'node:20-slim',
      demoR2Status: 'Running',
      demoR2Cpu: '1.2%',
      demoR2Mem: '128 MB',
      demoR3Name: 'db-postgres',
      demoR3Img: 'postgres:16',
      demoR3Status: 'Stopped',
      demoR3Cpu: '\u2014',
      demoR3Mem: '\u2014',
      demoR4Name: 'redis-cache',
      demoR4Img: 'redis:7-alpine',
      demoR4Status: 'Paused',
      demoR4Cpu: '\u2014',
      demoR4Mem: '52 MB',

      // Tech stack
      techLabel: 'Architecture',
      techTitle: 'Built with Rust. Powered by gRPC.',
      techDesc:
        'Full-stack Rust \u2014 from the GUI backend to the daemon to the VM engine. Zero garbage collection. Maximum performance.',

      // Comparison
      compLabel: 'Comparison',
      compTitle: 'How CrateBay stacks up.',
      compDesc: 'A quick look at how CrateBay compares to the alternatives.',
      compFeature: 'Feature',
      compCrateBay: 'CrateBay',
      compDocker: 'Docker Desktop',
      compOrbStack: 'OrbStack',
      compAppSize: 'App Size',
      compMemory: 'Memory Usage',
      compStartup: 'Startup Time',
      compOpenSource: 'Open Source',
      compFree: 'Free',
      compVM: 'VM Support',
      compK8s: 'Kubernetes',
      compK8sNote: '(K3s)',
      compPlugin: 'Plugin System',
      compCross: 'Cross-platform',

      compCBSize: '~18 MB',
      compDDSize: '~1.5 GB',
      compOSSize: '~200 MB',
      compCBMem: '~50 MB idle',
      compDDMem: '~2 GB',
      compOSMem: '~200 MB',
      compCBStart: '< 1s',
      compDDStart: '~10s',
      compOSStart: '~2s',

      // Getting started
      installLabel: 'Getting Started',
      installTitle: 'Install in seconds.',
      installDesc: 'Choose your preferred installation method.',
      installBrew: 'Homebrew',
      installBrewDesc: 'macOS & Linux',
      installCargo: 'Cargo',
      installCargoDesc: 'Build from source',
      installDirect: 'Direct Download',
      installDirectDesc: 'Pre-built binary',

      // Footer
      footerDocs: 'Documentation',
      footerRoadmap: 'Roadmap',
      footerLicense: 'License',
    },
    zh: {
      navFeatures: '功能',
      navDemo: '演示',
      navTech: '技术栈',
      navCompare: '对比',
      navInstall: '安装',
      themeToggle: '切换深色/浅色模式',
      heroBadge: 'v1.0.0 正式版',
      heroTitle1: 'Crate',
      heroTitle2: 'Bay',
      heroTagline: '开源桌面应用，统一管理 Docker 容器、Linux 虚拟机和 Kubernetes 集群 —— 极速，基于 Rust 构建。',
      heroDownload: '下载 macOS 版本',
      heroGithub: '在 GitHub 查看',
      statLang: '纯 Rust',
      statLicense: '开源',
      statSize: '应用大小',
      featLabel: '核心功能',
      featTitle: '容器管理，一应俱全。',
      featDesc: 'CrateBay 为 Docker 容器、Linux 虚拟机和 Kubernetes 集群提供统一、原生速度的管理体验——零额外开销。系统托盘提供快捷操作，并展示实时运行数量。',
      feat1Title: 'Docker 容器管理',
      feat1Desc: '全生命周期控制——创建、启动、停止、重启、删除。实时日志流和进入运行中容器的终端。端口映射信息在实时刷新时保持稳定显示。',
      feat2Title: 'Linux 虚拟机',
      feat2Desc: '通过 Virtualization.framework (macOS)、KVM (Linux)、Hyper-V (Windows) 原生支持虚拟机，秒级启动。',
      feat3Title: '镜像与卷管理',
      feat3Desc: '从 Docker Hub 和私有仓库搜索和拉取镜像。管理卷的检查、创建、清理和挂载操作。卷列表在实时刷新时保持稳定排序。',
      feat4Title: '资源监控',
      feat4Desc: '实时监控每个容器和虚拟机的 CPU、内存、网络和磁盘 I/O。即时发现问题。',
      feat5Title: '端口转发 & VirtioFS',
      feat5Desc: '自动端口转发，通过 VirtioFS 实现主机与客户机之间的近原生文件共享。',
      feat6Title: 'Kubernetes (K3s)',
      feat6Desc: '内置 K3s 集成，提供 Pod、Service 和 Deployment 仪表盘。轻松管理命名空间和工作负载。',
      feat7Title: '插件系统',
      feat7Desc: '可扩展架构与生命周期钩子。构建自定义插件来集成你自己的工具和工作流。',
      feat8Title: '跨平台',
      feat8Desc: '支持 macOS、Linux 和 Windows。相同的界面、相同的速度、相同的体验。',
      demoLabel: '界面',
      demoTitle: '为速度和清晰度而生。',
      demoDesc: '基于 Tauri 和 React 构建的简洁响应式 GUI。每次交互都瞬间完成。',
      demoTitlebar: 'CrateBay',
      demoSidebar0: '仪表盘',
      demoSidebar1: '容器',
      demoSidebar2: '镜像',
      demoSidebar3: '卷',
      demoSidebar4: '虚拟机',
      demoSidebar5: 'Kubernetes',
      demoHeader: '容器',
      demoBtn: '+ 新建',
      demoThName: '名称',
      demoThImage: '镜像',
      demoThStatus: '状态',
      demoThCpu: 'CPU',
      demoThMem: '内存',
      demoR1Name: 'web-frontend',
      demoR1Img: 'nginx:alpine',
      demoR1Status: '运行中',
      demoR1Cpu: '0.3%',
      demoR1Mem: '24 MB',
      demoR2Name: 'api-server',
      demoR2Img: 'node:20-slim',
      demoR2Status: '运行中',
      demoR2Cpu: '1.2%',
      demoR2Mem: '128 MB',
      demoR3Name: 'db-postgres',
      demoR3Img: 'postgres:16',
      demoR3Status: '已停止',
      demoR3Cpu: '\u2014',
      demoR3Mem: '\u2014',
      demoR4Name: 'redis-cache',
      demoR4Img: 'redis:7-alpine',
      demoR4Status: '已暂停',
      demoR4Cpu: '\u2014',
      demoR4Mem: '52 MB',
      techLabel: '架构',
      techTitle: 'Rust 构建，gRPC 驱动。',
      techDesc: '全栈 Rust——从 GUI 后端到守护进程再到 VM 引擎。零垃圾回收，极致性能。',
      compLabel: '对比',
      compTitle: 'CrateBay 横向对比。',
      compDesc: '快速了解 CrateBay 与同类产品的差异。',
      compFeature: '特性',
      compCrateBay: 'CrateBay',
      compDocker: 'Docker Desktop',
      compOrbStack: 'OrbStack',
      compAppSize: '应用大小',
      compMemory: '内存占用',
      compStartup: '启动时间',
      compOpenSource: '开源',
      compFree: '免费',
      compVM: '虚拟机支持',
      compK8s: 'Kubernetes',
      compK8sNote: '(K3s)',
      compPlugin: '插件系统',
      compCross: '跨平台',
      compCBSize: '~18 MB',
      compDDSize: '~1.5 GB',
      compOSSize: '~200 MB',
      compCBMem: '~50 MB 空闲',
      compDDMem: '~2 GB',
      compOSMem: '~200 MB',
      compCBStart: '< 1s',
      compDDStart: '~10s',
      compOSStart: '~2s',
      installLabel: '快速开始',
      installTitle: '秒级安装。',
      installDesc: '选择你喜欢的安装方式。',
      installBrew: 'Homebrew',
      installBrewDesc: 'macOS & Linux',
      installCargo: 'Cargo',
      installCargoDesc: '从源码构建',
      installDirect: '直接下载',
      installDirectDesc: '预编译二进制',
      footerDocs: '文档',
      footerRoadmap: '路线图',
      footerLicense: '许可证',
    },
  };

  let currentLang = 'en';

  function setLang(lang) {
    currentLang = lang;
    document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';

    // Update toggle buttons
    document.querySelectorAll('.lang-toggle button').forEach(function (btn) {
      btn.classList.toggle('active', btn.dataset.lang === lang);
    });

    // Update all [data-i18n] elements
    document.querySelectorAll('[data-i18n]').forEach(function (el) {
      var key = el.getAttribute('data-i18n');
      if (i18n[lang][key] !== undefined) {
        el.textContent = i18n[lang][key];
      }
    });

    // Update all [data-i18n-title] elements
    document.querySelectorAll('[data-i18n-title]').forEach(function (el) {
      var key = el.getAttribute('data-i18n-title');
      if (i18n[lang][key] !== undefined) {
        el.title = i18n[lang][key];
        el.setAttribute('aria-label', i18n[lang][key]);
      }
    });

    // Update [data-i18n-html] elements (for innerHTML)
    document.querySelectorAll('[data-i18n-html]').forEach(function (el) {
      var key = el.getAttribute('data-i18n-html');
      if (i18n[lang][key] !== undefined) {
        el.innerHTML = i18n[lang][key];
      }
    });
  }

  // ----------------------------------------------------------
  // Theme — Dark / Light mode
  // ----------------------------------------------------------
  function getSystemTheme() {
    if (window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches) {
      return 'light';
    }
    return 'dark';
  }

  function setTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('cratebay-theme', theme);
    // Update canvas colors if needed (handled by reading CSS vars in draw loop)
  }

  function initTheme() {
    var stored = localStorage.getItem('cratebay-theme');
    if (stored === 'light' || stored === 'dark') {
      setTheme(stored);
    } else {
      setTheme(getSystemTheme());
    }
  }

  function toggleTheme() {
    var current = document.documentElement.getAttribute('data-theme') || 'dark';
    setTheme(current === 'dark' ? 'light' : 'dark');
  }

  initTheme();

  // Bind theme toggle buttons
  document.querySelectorAll('.theme-toggle').forEach(function (btn) {
    btn.addEventListener('click', toggleTheme);
  });

  // ----------------------------------------------------------
  // Navbar scroll effect
  // ----------------------------------------------------------
  var navbar = document.querySelector('.navbar');
  function onScroll() {
    if (window.scrollY > 40) {
      navbar.classList.add('scrolled');
    } else {
      navbar.classList.remove('scrolled');
    }
  }
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  // ----------------------------------------------------------
  // Mobile menu
  // ----------------------------------------------------------
  var mobileBtn = document.querySelector('.mobile-menu-btn');
  var navLinks = document.querySelector('.nav-links');
  if (mobileBtn) {
    mobileBtn.addEventListener('click', function () {
      navLinks.classList.toggle('open');
    });
    // Close on link click
    navLinks.querySelectorAll('a').forEach(function (a) {
      a.addEventListener('click', function () {
        navLinks.classList.remove('open');
      });
    });
  }

  // ----------------------------------------------------------
  // Language toggle
  // ----------------------------------------------------------
  document.querySelectorAll('.lang-toggle button').forEach(function (btn) {
    btn.addEventListener('click', function () {
      setLang(btn.dataset.lang);
    });
  });

  // ----------------------------------------------------------
  // Copy code blocks
  // ----------------------------------------------------------
  document.querySelectorAll('.copy-btn').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var block = btn.closest('.code-block');
      var code = block.querySelector('code').textContent;
      navigator.clipboard.writeText(code.replace(/^\$ /gm, '').replace(/^# .*\n?/gm, '').trim()).then(function () {
        var original = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(function () {
          btn.textContent = original;
        }, 1500);
      });
    });
  });

  // ----------------------------------------------------------
  // Scroll reveal (IntersectionObserver)
  // ----------------------------------------------------------
  var reveals = document.querySelectorAll('.reveal');
  if ('IntersectionObserver' in window) {
    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add('visible');
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.12 }
    );
    reveals.forEach(function (el) { observer.observe(el); });
  } else {
    reveals.forEach(function (el) { el.classList.add('visible'); });
  }

  // ----------------------------------------------------------
  // Grid / particle canvas background
  // ----------------------------------------------------------
  var canvas = document.getElementById('grid-canvas');
  if (canvas) {
    var ctx = canvas.getContext('2d');
    var w, h;
    var particles = [];
    var PARTICLE_COUNT = 60;
    var GRID_SIZE = 60;
    var CONNECTION_DIST = 140;

    function resize() {
      w = canvas.width = window.innerWidth;
      h = canvas.height = window.innerHeight;
    }

    function initParticles() {
      particles = [];
      for (var i = 0; i < PARTICLE_COUNT; i++) {
        particles.push({
          x: Math.random() * w,
          y: Math.random() * h,
          vx: (Math.random() - 0.5) * 0.3,
          vy: (Math.random() - 0.5) * 0.3,
          r: Math.random() * 1.5 + 0.5,
        });
      }
    }

    function getCanvasColors() {
      var style = getComputedStyle(document.documentElement);
      return {
        grid: style.getPropertyValue('--canvas-grid').trim(),
        particle: style.getPropertyValue('--canvas-particle').trim(),
        line: style.getPropertyValue('--canvas-line').trim(),
      };
    }

    function drawGrid(colors) {
      ctx.strokeStyle = colors.grid;
      ctx.lineWidth = 0.5;
      for (var x = 0; x < w; x += GRID_SIZE) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }
      for (var y = 0; y < h; y += GRID_SIZE) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }
    }

    function drawParticles(colors) {
      for (var i = 0; i < particles.length; i++) {
        var p = particles[i];
        // Move
        p.x += p.vx;
        p.y += p.vy;
        if (p.x < 0) p.x = w;
        if (p.x > w) p.x = 0;
        if (p.y < 0) p.y = h;
        if (p.y > h) p.y = 0;

        // Draw dot
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
        ctx.fillStyle = colors.particle;
        ctx.fill();

        // Connections
        for (var j = i + 1; j < particles.length; j++) {
          var q = particles[j];
          var dx = p.x - q.x;
          var dy = p.y - q.y;
          var dist = Math.sqrt(dx * dx + dy * dy);
          if (dist < CONNECTION_DIST) {
            ctx.beginPath();
            ctx.moveTo(p.x, p.y);
            ctx.lineTo(q.x, q.y);
            ctx.strokeStyle = 'rgba(' + colors.line + ', ' +
              (0.12 * (1 - dist / CONNECTION_DIST)) + ')';
            ctx.lineWidth = 0.5;
            ctx.stroke();
          }
        }
      }
    }

    function animate() {
      ctx.clearRect(0, 0, w, h);
      var colors = getCanvasColors();
      drawGrid(colors);
      drawParticles(colors);
      requestAnimationFrame(animate);
    }

    resize();
    initParticles();
    animate();

    window.addEventListener('resize', function () {
      resize();
      initParticles();
    });
  }

  // Initialize language
  setLang('en');
})();
