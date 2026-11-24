import { createFileRoute } from "@tanstack/react-router";
import { canonical } from "@/lib/site";
import { ImportWorkspace } from "@/components/support-workspaces";
export const Route = createFileRoute("/import")({
  head: () => ({
    meta: [
      { title: "Import candidates | ledger" },
      { name: "description", content: "Map and import candidate CSV exports from your ATS." },
      { property: "og:title", content: "Import candidates | ledger" },
      {
        property: "og:description",
        content: "Map and import candidate CSV exports from your ATS.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
      ...canonical("/import").meta,
    ],
    links: canonical("/import").links,
  }),
  component: ImportWorkspace,
});
