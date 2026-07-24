import { useState } from "react";
import { DocumentManager } from "./DocumentManager";

export function DocsTab() {
  const [helpOpen, setHelpOpen] = useState(false);

  return (
    <div className="max-w-3xl mx-auto py-8 space-y-8">
      <h2 className="text-lg font-semibold text-text-primary">
        Documentatie &mdash; Teun
      </h2>

      <DocumentManager />

      <div className="border-t border-border pt-6">
        <button
          onClick={() => setHelpOpen((open) => !open)}
          className="text-xs font-medium text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
        >
          {helpOpen ? "▾ Verberg help & uitleg" : "▸ Toon help & uitleg"}
        </button>

        {helpOpen && (
          <div className="mt-6 space-y-8">
        <Section title="Wat is Teun?">
          <p>
            Teun is jouw geheugensteun. Stel een vraag en Teun zoekt het voor je uit
            in beleid, acceptatiekader en handleidingen van MUNT. Teun doorzoekt
            de beleidsdocumenten en geeft een onderbouwd antwoord met
            bronvermelding.
          </p>
          <p>
            De antwoorden worden gegenereerd door AI en zijn <strong>niet
            bindend</strong>. Teun neemt geen besluiten &mdash; hij
            presenteert uitsluitend informatie uit het beleid.
          </p>
        </Section>

        <Section title="Hoe gebruik je Teun?">
          <ol className="list-decimal list-inside space-y-2">
            <li>
              Stel je vraag in het invoerveld onderaan het scherm.
            </li>
            <li>
              Teun doorzoekt de beleidsdocumenten en geeft een antwoord
              met bronvermelding.
            </li>
            <li>
              Controleer de <strong>betrouwbaarheidsscore</strong> bij het
              antwoord. Klik op &ldquo;Toon details&rdquo; om de onderbouwing
              en bronverificatie in te zien.
            </li>
            <li>
              Geef feedback op het antwoord via de feedbackknoppen (goedgekeurd,
              gedeeltelijk correct, of afgekeurd).
            </li>
          </ol>
        </Section>

        <Section title="Betrouwbaarheidsscore">
          <p>
            Elk antwoord wordt automatisch beoordeeld door een tweede AI-model
            (de &ldquo;judge&rdquo;). De score geeft aan hoe betrouwbaar het
            antwoord is op een schaal van 1 tot 100:
          </p>
          <ul className="space-y-2">
            <li className="flex items-start gap-2">
              <span className="inline-block mt-0.5 h-3 w-3 rounded-full bg-green-500 shrink-0" />
              <span>
                <strong>Groen (boven hoge drempel)</strong> &mdash; Het antwoord
                is betrouwbaar en goed onderbouwd.
              </span>
            </li>
            <li className="flex items-start gap-2">
              <span className="inline-block mt-0.5 h-3 w-3 rounded-full bg-amber-500 shrink-0" />
              <span>
                <strong>Oranje (tussen drempels)</strong> &mdash; Beperkte
                betrouwbaarheid. Extra verificatie van bronnen is vereist.
              </span>
            </li>
            <li className="flex items-start gap-2">
              <span className="inline-block mt-0.5 h-3 w-3 rounded-full bg-red-500 shrink-0" />
              <span>
                <strong>Rood (onder lage drempel)</strong> &mdash; Onbetrouwbaar.
                Raadpleeg team Acceptatie voor dit onderwerp.
              </span>
            </li>
          </ul>
          <p>
            De drempelwaarden zijn instelbaar via het tabblad Instellingen.
          </p>
        </Section>

        <Section title="Chat modi">
          <p>
            Teun ondersteunt twee werkwijzen, instelbaar via Instellingen:
          </p>
          <ul className="space-y-2">
            <li>
              <strong>Tools modus</strong> &mdash; Teun doorzoekt de
              beleidsdocumenten actief met zoek- en leestools. Geeft nauwkeurige
              bronverwijzingen met regelnummers.
            </li>
            <li>
              <strong>Inline modus</strong> &mdash; Alle beleidsdocumenten worden
              vooraf geladen. Snellere antwoorden, maar minder gedetailleerde
              bronverwijzingen.
            </li>
          </ul>
        </Section>

        <Section title="PII-bescherming">
          <p>
            Wanneer PII-bescherming is ingeschakeld, worden persoonsgegevens
            (BSN, namen, adressen) automatisch gedetecteerd en geanonimiseerd
            voordat het bericht naar Teun wordt gestuurd. Je ziet een
            voorbeeldweergave van de geanonimiseerde tekst voordat het bericht
            wordt verzonden.
          </p>
        </Section>

        <Section title="Gesprekken en opslag">
          <p>
            Gesprekken worden opgeslagen voor monitoring en verbetering. Eerdere
            gesprekken zijn terug te vinden via de zijbalk aan de linkerkant van
            het scherm.
          </p>
          <p>
            Gesprekken worden na de retentieperiode automatisch verwijderd.
          </p>
        </Section>

        <Section title="Taal">
          <p>
            De standaardtaal is Nederlands. Via Instellingen kun je de taal
            wijzigen naar Engels. Teun past de taal van de antwoorden
            aan op basis van deze instelling.
          </p>
        </Section>

        <Section title="Hulp nodig?">
          <p>
            Neem bij vragen of problemen contact op met team Acceptatie.
          </p>
        </Section>
          </div>
        )}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-3">
      <h3 className="text-sm font-semibold text-text-primary">{title}</h3>
      <div className="text-sm text-text-secondary leading-relaxed space-y-2">
        {children}
      </div>
    </section>
  );
}
