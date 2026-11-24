import { createFileRoute } from "@tanstack/react-router";
import { canonical } from "@/lib/site";
import { AuditWorkspace } from "@/components/support-workspaces";
export const Route = createFileRoute("/audit")({
  head: () => ({
    meta: [
      { title: "Audit trail | ledger" },
      {
        name: "description",
        content: "Inspect preview activity and export a recruitment workspace snapshot.",
      },
      { property: "og:title", content: "Audit trail | ledger" },
      {
        property: "og:description",
        content: "Inspect preview activity and export a recruitment workspace snapshot.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
      ...canonical("/audit").meta,
    ],
    links: canonical("/audit").links,
  }),
  component: AuditWorkspace,
});
