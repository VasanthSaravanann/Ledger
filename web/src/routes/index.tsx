import { createFileRoute } from "@tanstack/react-router";
import { canonical } from "@/lib/site";
import { LandingPage } from "@/components/landing-page";
export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "Ledger — every rejection leaves a record" },
      {
        name: "description",
        content:
          "A manual candidate-feedback pilot: one employer, one requisition, every rejected candidate told why, in copy Legal signed, with a log that proves the exact text sent.",
      },
      { property: "og:title", content: "Ledger — every rejection leaves a record" },
      {
        property: "og:description",
        content:
          "A manual candidate-feedback pilot: a reason a person recorded, copy Legal signed, and an append-only log that proves the exact text sent.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
      ...canonical("/").meta,
    ],
    links: canonical("/").links,
  }),
  component: LandingPage,
});
