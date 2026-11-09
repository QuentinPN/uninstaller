// Every user-facing label lives here (CLAUDE.md): components never hard-code text.
export const fr = {
  common: {
    retry: "Réessayer",
  },
  window: {
    minimize: "Réduire",
    maximize: "Agrandir",
    restore: "Niveau inférieur",
    close: "Fermer",
    elevated: "Administrateur",
    elevatedHint:
      "L'application tourne en administrateur, vue web comprise. Relancez-la normalement : les opérations qui en ont besoin demanderont l'autorisation.",
  },
  nav: {
    label: "Sections",
    programs: "Programmes",
    apps: "Applications Windows",
    extensions: "Extensions",
    tools: "Outils",
    backups: "Sauvegardes",
    tracking: "Suivi",
    settings: "Réglages",
  },
  programs: {
    count: (n: number) => `${n} programme${n > 1 ? "s" : ""}`,
    selected: (n: number) => `${n} sélectionné${n > 1 ? "s" : ""}`,
    sizing: "Calcul des tailles…",
    search: "Rechercher",
    searchHint: "Rechercher par nom, éditeur, commentaire ou dossier",
    sortBy: "Trier par",
    list: "Programmes installés",
    columns: "Colonnes",
    /** Sort keys and columns. */
    field: {
      name: "Nom",
      version: "Version",
      size: "Taille",
      date: "Date d'installation",
      scope: "Installation",
    },
    /** Column headings shorter than their field name. */
    columnHead: { date: "Installé le" } as Partial<Record<string, string>>,
    sortHint: (field: string) =>
      `Trier par ${field.toLowerCase()} ; recliquer inverse l'ordre`,
    export: {
      button: "Exporter",
      /** File types offered, in the menu and the save dialog. */
      formats: { html: "Page HTML", csv: "Tableur CSV", txt: "Texte" },
      dialogTitle: "Exporter la liste des programmes",
      fileName: (date: string) => `Programmes ${date}`,
      exportedOn: (date: string) => `exporté le ${date}`,
      saved: (path: string) => `Liste exportée : ${path}`,
      failed: "L'export n'a pas pu être enregistré",
      print: "Imprimer…",
      printedOn: (date: string) => `imprimé le ${date}`,
    },
    viewAs: "Vue",
    views: { details: "Détails", icons: "Icônes", list: "Liste" },
    /** On « Colonnes » outside the Détails view. */
    columnsDetailsOnly: "Les colonnes s'affichent dans la vue Détails",
    groupBy: "Regrouper",
    group: {
      none: "Aucun",
      publisher: "Éditeur",
      date: "Mois d'installation",
      size: "Taille",
    },
    /** Group headings: unknown values, and size ranges from the largest. */
    groupUnknown: {
      publisher: "Éditeur inconnu",
      date: "Date inconnue",
      size: "Taille inconnue",
    },
    sizeRanges: [
      "1 Go et plus",
      "100 Mo à 1 Go",
      "10 à 100 Mo",
      "Moins de 10 Mo",
    ],
    filter: "Filtrer",
    filters: { recent: "Récents", orphan: "Orphelins" },
    recentHint: (days: number) => `Installés ces ${days} derniers jours`,
    orphanHint: "Le désinstalleur ou le dossier du programme n'existe plus",
    /** Tag on an orphan's row. */
    orphan: "Absent du disque",
    show: "Afficher",
    /** Toggles for the kinds hidden by default, and the tag on their rows. */
    kinds: { systemComponent: "Composants système", update: "Mises à jour" },
    kind: { systemComponent: "Composant système", update: "Mise à jour" },
    scope: {
      machine64: "64 bits",
      machine32: "32 bits",
      user: "Utilisateur",
      otherUser: "Autre compte",
    },
    scopeHint: {
      machine64: "Installé pour tous les utilisateurs, 64 bits",
      machine32: "Installé pour tous les utilisateurs, 32 bits",
      user: "Installé pour votre compte seulement",
      otherUser: "Installé pour un autre compte de ce PC",
    },
    noResults: {
      title: "Aucun programme trouvé",
      text: "Essayez un autre nom, un éditeur ou un morceau du dossier d'installation.",
    },
    error: {
      title: "Impossible de lire la liste des programmes",
      text: "Le registre n'a pas pu être lu. Réessayez ; si cela persiste, redémarrez l'application.",
    },
    details: {
      label: "Détails du programme",
      close: "Fermer les détails (Échap)",
      version: "Version",
      diskSize: "Taille sur le disque",
      size: "Taille déclarée",
      installed: "Installé le",
      scope: "Installation",
      account: "Compte",
      installer: "Installeur",
      msi: "Windows Installer",
      comments: "Commentaire",
      folder: "Dossier",
      site: "Site",
      uninstall: "Commande de désinstallation",
      quietUninstall: "Désinstallation silencieuse",
      registryKey: "Clé de registre",
      copy: "Copier",
      copyField: (label: string) => `Copier : ${label}`,
      copied: "Copié",
      copyFailed: "Impossible de copier",
      openFolder: "Ouvrir le dossier",
      folderMissing: "Ce dossier n'existe plus",
    },
  },
  backups: {
    count: (n: number) => `${n} lot${n > 1 ? "s" : ""} en quarantaine`,
    quarantine: "Quarantaine",
    /** What a batch still holds: "12 éléments · 3 clés" (files, folders and links are elements). */
    contents: (files: number, keys: number) => {
      const parts = [];
      if (files) parts.push(`${files} élément${files > 1 ? "s" : ""}`);
      if (keys) parts.push(`${keys} clé${keys > 1 ? "s" : ""}`);
      return parts.length ? parts.join(" · ") : "Tout restauré";
    },
    purgeOn: (day: string) => `Purge le ${day}`,
    purging: "Purge en cours",
    restore: "Restaurer",
    purge: "Purger",
    cancel: "Annuler",
    /** The purge confirmation repeats what goes (DESIGN.md: no danger without a count). */
    confirmPurge: (contents: string, size: string) =>
      `Supprimer définitivement ${contents} · ${size} ? Ils ne pourront plus être restaurés.`,
    createPoint: "Créer un point de restauration",
    creatingPoint: "Création du point de restauration…",
    /** After « Créer un point de restauration », by `RestorePoint` or `ElevationFailure`. */
    point: {
      created: "Point de restauration créé",
      skipped:
        "Windows n'a pas créé de point : le précédent est trop récent. Il reste utilisable.",
      disabled:
        "La protection du système est désactivée : aucun point n'a été créé.",
      refused: "Autorisation administrateur refusée : rien n'a été modifié.",
      failed: "Le point de restauration n'a pas pu être créé.",
    },
    restored: (name: string) => `${name} restauré`,
    purged: (name: string) => `${name} purgé`,
    /** Why an action stopped, by `BatchFailure`. */
    failure: {
      missing: "Ce lot n'existe plus.",
      exists:
        "Un élément est revenu à son emplacement d'origine : rien n'a été écrasé. Les éléments déjà remis en place le restent.",
      altered:
        "Un élément a changé depuis sa mise en quarantaine : il n'a pas été restauré.",
      corrupt: "Le manifeste de ce lot est endommagé : rien n'a été modifié.",
      purging: "Ce lot est en cours de purge.",
      refused:
        "Autorisation administrateur refusée : les éléments déjà remis en place le restent, rien d'autre n'a été modifié.",
      failed:
        "L'opération a échoué. Réessayez ; si cela persiste, redémarrez l'application.",
    },
    empty: {
      title: "Rien en quarantaine",
      text: "Ce qu'un nettoyage retire arrive ici et reste restaurable jusqu'à sa purge.",
    },
    error: {
      title: "Impossible de lire la quarantaine",
      text: "Le dossier de quarantaine n'a pas pu être lu. Réessayez ; si cela persiste, redémarrez l'application.",
    },
  },
  /** Screens not built yet: what will be there, and when. */
  upcoming: {
    title: "Pas encore là",
    apps: "Phase 5 : les applications du Microsoft Store et celles préinstallées.",
    extensions: "Phase 5 : les extensions de Chrome, Edge, Brave et Firefox.",
    tools:
      "Phase 7 : démarrage, fichiers inutiles, navigateurs, historiques, broyeur et outils Windows.",
    tracking:
      "Phase 8 : suivre une installation pour la défaire entièrement plus tard.",
    settings: "Les réglages arrivent avec les fonctions qui en ont besoin.",
  },
} as const;
