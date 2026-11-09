// Every user-facing label lives here (CLAUDE.md): components never hard-code text.
export const fr = {
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
    search: "Rechercher",
    searchHint: "Rechercher par nom, éditeur, commentaire ou dossier",
    sortBy: "Trier par",
    sort: { name: "Nom", size: "Taille", date: "Date d'installation" },
    list: "Programmes installés",
    scope: { machine64: "64 bits", machine32: "32 bits", user: "Utilisateur" },
    scopeHint: {
      machine64: "Installé pour tous les utilisateurs, 64 bits",
      machine32: "Installé pour tous les utilisateurs, 32 bits",
      user: "Installé pour votre compte seulement",
    },
    noResults: {
      title: "Aucun programme trouvé",
      text: "Essayez un autre nom, un éditeur ou un morceau du dossier d'installation.",
    },
    error: {
      title: "Impossible de lire la liste des programmes",
      text: "Le registre n'a pas pu être lu. Réessayez ; si cela persiste, redémarrez l'application.",
      retry: "Réessayer",
    },
  },
  /** Screens not built yet: what will be there, and when. */
  upcoming: {
    title: "Pas encore là",
    apps: "Phase 5 : les applications du Microsoft Store et celles préinstallées.",
    extensions: "Phase 5 : les extensions de Chrome, Edge, Brave et Firefox.",
    tools:
      "Phase 7 : démarrage, fichiers inutiles, navigateurs, historiques, broyeur et outils Windows.",
    backups:
      "Phase 2 : points de restauration, sauvegardes du registre et éléments en quarantaine.",
    tracking:
      "Phase 8 : suivre une installation pour la défaire entièrement plus tard.",
    settings: "Les réglages arrivent avec les fonctions qui en ont besoin.",
  },
} as const;
