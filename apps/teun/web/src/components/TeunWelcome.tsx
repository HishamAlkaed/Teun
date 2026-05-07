import { useState } from "react";

interface TeunWelcomeProps {
  onSend: (message: string) => void;
}

const exampleQuestions = [
  {
    tag: "Acceptatie",
    question: "Kan een woning met erfpacht als onderpand dienen?",
    body: "Teun kan uitzoeken of een specifieke erfpachtconstructie past binnen het acceptatiekader van MUNT — inclusief voorwaarden over eeuwigdurende erfpacht, canonherziening en gemeentelijke voorwaarden.",
  },
  {
    tag: "Beleid",
    question: "Wat is het beleid rondom overbruggingshypotheken?",
    body: "Stel vragen over voorwaarden, looptijden en beperkingen van overbruggingshypotheken. Teun doorzoekt het beleid en geeft je een helder antwoord met bronvermelding.",
  },
  {
    tag: "Handleiding",
    question: "Hoe voer ik een BKR-toetsing uit in het systeem?",
    body: "Teun helpt je stap voor stap door processen en systemen. Van BKR-toetsingen tot het verwerken van taxatierapporten — snel en duidelijk uitgelegd.",
  },
  {
    tag: "Voorwaarden",
    question: "Mag een klant boetevrij extra aflossen bij MUNT?",
    body: "Vragen over productvoorwaarden? Teun zoekt de exacte bepalingen op in de gids — inclusief percentages, uitzonderingen en beperkingen.",
  },
];

export function TeunWelcome({ onSend }: TeunWelcomeProps) {
  const [openAccordion, setOpenAccordion] = useState<number | null>(null);

  const toggleAccordion = (index: number) => {
    setOpenAccordion(openAccordion === index ? null : index);
  };

  return (
    <div className="flex flex-col items-center justify-center py-14 px-4 max-w-[960px] mx-auto">
      {/* Badge */}
      <div className="animate-fade-down inline-flex items-center gap-2 px-[18px] py-2 bg-surface border border-border rounded-full text-[11px] font-bold tracking-[0.1em] uppercase text-gray mb-10">
        <span className="w-[7px] h-[7px] rounded-full bg-accent" />
        MUNT Hypotheken
      </div>

      {/* Logo */}
      <svg className="w-[140px] h-[140px] mb-10 animate-logo-in" viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M32 8 H168 C182 8 192 18 192 32 V128 C192 142 182 152 168 152 H56 L24 180 V152 C14 152 8 142 8 128 V32 C8 18 18 8 32 8 Z" fill="url(#lg)" />
        <rect x="36" y="34" width="64" height="80" rx="8" fill="white" />
        <rect x="50" y="52" width="34" height="3" rx="1.5" fill="#00D696" opacity="0.45" />
        <rect x="50" y="62" width="24" height="3" rx="1.5" fill="#00D696" opacity="0.3" />
        <rect x="50" y="72" width="30" height="3" rx="1.5" fill="#00D696" opacity="0.45" />
        <rect x="50" y="82" width="20" height="3" rx="1.5" fill="#00D696" opacity="0.3" />
        <rect x="50" y="92" width="28" height="3" rx="1.5" fill="#00D696" opacity="0.2" />
        <circle cx="128" cy="76" r="32" fill="white" />
        <circle cx="124" cy="72" r="18" fill="none" stroke="#00D696" strokeWidth="4" />
        <line x1="137" y1="85" x2="150" y2="98" stroke="#00D696" strokeWidth="4" strokeLinecap="round" />
        <circle cx="80" cy="132" r="5" fill="white" opacity="0.4">
          <animate attributeName="opacity" values="0.25;0.65;0.25" dur="1.4s" repeatCount="indefinite" />
        </circle>
        <circle cx="100" cy="132" r="5" fill="white" opacity="0.3">
          <animate attributeName="opacity" values="0.25;0.65;0.25" dur="1.4s" begin="0.25s" repeatCount="indefinite" />
        </circle>
        <circle cx="120" cy="132" r="5" fill="white" opacity="0.2">
          <animate attributeName="opacity" values="0.25;0.65;0.25" dur="1.4s" begin="0.5s" repeatCount="indefinite" />
        </circle>
        <defs>
          <linearGradient id="lg" x1="8" y1="8" x2="192" y2="180" gradientUnits="userSpaceOnUse">
            <stop offset="0%" stopColor="#00D696" />
            <stop offset="100%" stopColor="#00C48A" />
          </linearGradient>
        </defs>
      </svg>

      {/* Heading */}
      <h1 className="animate-fade-up font-serif text-[clamp(34px,5.5vw,48px)] font-bold leading-[1.1] text-center uppercase tracking-[0.02em] mb-3 text-text-primary" style={{ animationDelay: "0.3s" }}>
        Maak kennis met <span className="text-accent">Teun.</span>
      </h1>

      {/* Subtitle */}
      <p className="animate-fade-up text-[17px] text-gray text-center mb-12 font-normal leading-[1.6]" style={{ animationDelay: "0.4s" }}>
        Jouw geheugensteun. Stel een vraag en Teun zoekt het voor je uit.
        <br />
        In beleid, acceptatiekader en handleidingen.
      </p>

      {/* Chat preview + Reasoning sidebar */}
      <div className="animate-fade-up flex gap-4 w-full max-w-[840px] mb-12 items-start relative max-md:flex-col" style={{ animationDelay: "0.5s" }}>
        {/* Chat card */}
        <div className="flex-1 min-w-0 bg-surface rounded-xl border border-border overflow-hidden shadow-[0_1px_4px_rgba(0,0,0,0.06)]">
          {/* Chat header */}
          <div className="flex items-center gap-3 px-6 py-4 border-b border-border">
            <svg className="w-[34px] h-[34px] shrink-0" viewBox="0 0 200 200" fill="none">
              <path d="M32 8 H168 C182 8 192 18 192 32 V128 C192 142 182 152 168 152 H56 L24 180 V152 C14 152 8 142 8 128 V32 C8 18 18 8 32 8 Z" fill="url(#lgs)" />
              <rect x="36" y="34" width="58" height="72" rx="8" fill="white" />
              <rect x="50" y="54" width="30" height="4" rx="2" fill="#00D696" opacity="0.45" />
              <rect x="50" y="66" width="22" height="4" rx="2" fill="#00D696" opacity="0.3" />
              <rect x="50" y="78" width="26" height="4" rx="2" fill="#00D696" opacity="0.45" />
              <circle cx="128" cy="74" r="28" fill="white" />
              <circle cx="124" cy="70" r="15" fill="none" stroke="#00D696" strokeWidth="4.5" />
              <line x1="135" y1="81" x2="146" y2="92" stroke="#00D696" strokeWidth="4.5" strokeLinecap="round" />
              <circle cx="78" cy="130" r="6" fill="white" opacity="0.3" />
              <circle cx="98" cy="130" r="6" fill="white" opacity="0.2" />
              <circle cx="118" cy="130" r="6" fill="white" opacity="0.1" />
              <defs>
                <linearGradient id="lgs" x1="8" y1="8" x2="192" y2="180" gradientUnits="userSpaceOnUse">
                  <stop offset="0%" stopColor="#00D696" />
                  <stop offset="100%" stopColor="#00C48A" />
                </linearGradient>
              </defs>
            </svg>
            <div>
              <div className="font-bold text-[15px] text-text-primary">Teun</div>
              <div className="text-xs text-accent-dark flex items-center gap-[5px]">
                <span className="w-1.5 h-1.5 rounded-full bg-accent" />
                Altijd beschikbaar voor jou
              </div>
            </div>
          </div>

          {/* Chat body */}
          <div className="p-6 flex flex-col gap-3.5">
            <div className="animate-msg-in self-end max-w-[85%] px-4 py-3 text-sm leading-[1.55] rounded-xl rounded-br bg-bg-light text-text-primary" style={{ animationDelay: "0.8s" }}>
              De woning van de klant is houtskeletbouw uit 1987. Voldoet dit aan de acceptatievoorwaarden voor een hypotheek?
            </div>
            <div className="animate-msg-in self-start max-w-[85%] px-4 py-3 text-sm leading-[1.55] rounded-xl rounded-bl bg-text-primary text-white/[0.92]" style={{ animationDelay: "1.2s" }}>
              Nee, een woning met houtskeletbouw uit 1987 voldoet <strong className="text-accent font-semibold">NIET</strong> aan de acceptatievoorwaarden. MUNT accepteert houten woningen niet als onderpand, behalve voor prefab, houtskeletbouw of CLT woningen die een betonnen fundering hebben en na 2012 zijn gebouwd. Omdat de woning van je klant uit 1987 stamt, valt deze buiten de acceptabele bouwperiode.
            </div>
          </div>

          {/* Chat input */}
          <div className="flex items-center gap-2.5 px-6 py-3.5 border-t border-border">
            <span className="flex-1 text-[13px] text-gray-light">Stel je vraag aan Teun...</span>
            <div className="w-[34px] h-[34px] rounded-lg bg-accent flex items-center justify-center shrink-0 cursor-pointer hover:bg-accent-dark transition-colors">
              <svg className="w-4 h-4 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <line x1="5" y1="12" x2="19" y2="12" />
                <polyline points="12 5 19 12 12 19" />
              </svg>
            </div>
          </div>
        </div>

        {/* Reasoning sidebar */}
        <div className="animate-brain-in w-[230px] shrink-0 max-md:w-full">
          <div className="border border-border rounded-[14px] bg-surface shadow-[0_2px_12px_rgba(0,214,150,0.06),0_1px_4px_rgba(0,0,0,0.05)] p-5 flex flex-col gap-3.5 relative overflow-hidden">
            {/* Green top accent */}
            <div className="absolute top-0 left-0 right-0 h-[3px] bg-gradient-to-r from-accent to-accent-dark rounded-t-[14px]" />

            {/* Header */}
            <div className="flex items-center gap-2.5">
              <div className="w-8 h-8 rounded-[9px] bg-green-soft flex items-center justify-center shrink-0 relative">
                <svg className="text-accent-dark" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M9.5 2A5.5 5.5 0 0 0 4 7.5c0 1.58.67 3 1.74 4.01L4 13v2h2l.5-.5C7.84 15.44 9.13 16 10.5 16h.5v5h3v-5h.5c1.37 0 2.66-.56 3.5-1.5l.5.5h2v-2l-1.74-1.49A5.48 5.48 0 0 0 20.5 7.5 5.5 5.5 0 0 0 15 2h-.5a5.5 5.5 0 0 0-5 0Z" />
                  <path d="M12 2v5" />
                  <path d="M8.5 7h7" />
                </svg>
                <span className="absolute -inset-[3px] rounded-[12px] border-[1.5px] border-accent opacity-0 animate-brain-pulse" />
              </div>
              <div className="flex flex-col gap-px">
                <span className="text-[13px] font-bold text-text-primary">Het brein van Teun</span>
                <span className="text-[10px] text-gray font-normal">Zo komt het antwoord tot stand</span>
              </div>
            </div>

            {/* Reasoning steps */}
            <div className="flex flex-col relative">
              <div className="absolute left-2 top-[18px] bottom-[18px] w-[1.5px] bg-bg-light" />
              {[
                { num: "1", text: 'Zoeken naar "houtskeletbouw" in acceptatiekader' },
                { num: "2", text: <>Bouwjaar 1987 vergeleken met vereiste: <em className="text-accent-dark not-italic">na 2012</em></> },
                { num: "3", text: "Conclusie: voldoet niet aan criteria" },
              ].map((step, i) => (
                <div key={i} className="flex items-start gap-2.5 text-[11.5px] text-dark-soft leading-[1.5] py-1.5 relative">
                  <span className={`shrink-0 w-[18px] h-[18px] rounded-full flex items-center justify-center text-[9px] font-bold z-[1] ${i === 2 ? "bg-green-soft text-accent-dark" : "bg-bg-light text-gray"}`}>
                    {step.num}
                  </span>
                  <span>{step.text}</span>
                </div>
              ))}
            </div>

            <div className="h-px bg-border" />

            {/* Sources */}
            <div>
              <div className="text-[9px] font-bold tracking-[0.1em] text-gray-light mb-1.5">BRONNEN</div>
              <div className="flex items-center gap-1.5 text-[11px] font-medium text-text-primary bg-bg-light rounded-md px-2.5 py-[7px] mb-1 cursor-pointer hover:bg-border transition-colors">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#888" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                </svg>
                <span className="flex-1 min-w-0 truncate">MUNT_Hypotheekgids_2025-002</span>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="#BBB" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <polyline points="9 18 15 12 9 6" />
                </svg>
              </div>
              <div className="text-[10px] text-gray pl-2 mb-2">Acceptatiecriteria onderpand — r. 1996–1999</div>
            </div>

            <div className="h-px bg-border" />

            {/* Score */}
            <div className="flex items-center justify-between">
              <div className="inline-flex items-center gap-1 text-[11px] font-bold text-accent-dark">
                <span className="bg-accent text-white px-[7px] py-[3px] rounded-[5px] text-[11px] font-extrabold">92</span>
                Betrouwbaar
              </div>
              <span className="text-[10px] text-gray-light cursor-pointer">Toon details</span>
            </div>
          </div>
        </div>
      </div>

      {/* Example questions accordion */}
      <div className="animate-fade-up w-full max-w-[840px] mb-12" style={{ animationDelay: "0.85s" }}>
        <div className="flex items-center gap-2 text-lg font-bold text-text-primary mb-3 font-sans">
          <svg className="text-accent" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" />
            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
          Wat kun je Teun vragen?
        </div>
        <div className="flex flex-col gap-1.5">
          {exampleQuestions.map((item, index) => (
            <div key={index} className="bg-surface border border-border rounded-[10px] overflow-hidden shadow-[0_1px_3px_rgba(0,0,0,0.04)] transition-colors hover:border-accent">
              <div
                className="flex items-center gap-2.5 px-[18px] py-3.5 cursor-pointer select-none text-sm font-semibold text-text-primary hover:bg-page-bg transition-colors"
                onClick={() => toggleAccordion(index)}
              >
                <span className="text-[9px] font-bold tracking-[0.08em] uppercase px-2 py-[3px] rounded bg-green-soft text-accent-dark shrink-0">
                  {item.tag}
                </span>
                <span className="flex-1">{item.question}</span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onSend(item.question);
                  }}
                  className="shrink-0 text-[10px] font-semibold text-accent hover:text-accent-dark transition-colors px-2 py-1 rounded hover:bg-green-soft"
                  title="Stel deze vraag"
                >
                  Vraag &rarr;
                </button>
                <svg
                  className={`shrink-0 text-gray-light transition-transform duration-200 ${openAccordion === index ? "rotate-180" : ""}`}
                  width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"
                >
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </div>
              <div
                className="overflow-hidden transition-all duration-300"
                style={{ maxHeight: openAccordion === index ? "200px" : "0px" }}
              >
                <div className="px-[18px] pb-4 text-[13px] text-dark-soft leading-[1.6]">
                  {item.body}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Features grid */}
      <div className="animate-fade-up grid grid-cols-3 gap-4 w-full max-w-[840px] mb-14 max-md:grid-cols-1 max-md:max-w-[340px]" style={{ animationDelay: "1.0s" }}>
        {[
          {
            icon: (
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#00D696" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
              </svg>
            ),
            title: "Beleid doorzoeken",
            desc: "Handleidingen, regels en documenten — in een zoekvraag.",
          },
          {
            icon: (
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#00D696" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
              </svg>
            ),
            title: "Direct antwoord",
            desc: "Geen lange zoektochten. Gewoon snel het juiste antwoord.",
          },
          {
            icon: (
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#00D696" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" /><polyline points="22 4 12 14.01 9 11.01" />
              </svg>
            ),
            title: "Regel checken",
            desc: "Twijfel over een regel? Vraag het Teun. Zo simpel is het.",
          },
        ].map((feature, i) => (
          <div key={i} className="text-center p-6 rounded-xl border border-border bg-surface hover:border-accent transition-colors shadow-[0_1px_4px_rgba(0,0,0,0.06)]">
            <div className="w-[42px] h-[42px] rounded-[10px] bg-bg-light flex items-center justify-center mx-auto mb-3">
              {feature.icon}
            </div>
            <div className="font-bold text-sm mb-1 text-text-primary">{feature.title}</div>
            <div className="text-xs text-gray leading-[1.5]">{feature.desc}</div>
          </div>
        ))}
      </div>

      {/* Tagline */}
      <div className="animate-fade-up text-center" style={{ animationDelay: "1.2s" }}>
        <div className="w-10 h-[3px] bg-accent rounded-sm mx-auto mb-5" />
        <p className="font-sans text-[22px] font-bold uppercase tracking-[0.02em] text-text-primary mb-1">
          <span className="text-accent">Teun.</span> Jouw geheugensteun.
        </p>
        <p className="text-[15px] text-gray font-normal">Alles wat je nodig hebt, binnen handbereik.</p>
      </div>

      {/* Disclaimer */}
      <div className="animate-fade-up mt-10 max-w-[520px] text-center" style={{ animationDelay: "1.3s" }}>
        <div className="inline-flex items-start gap-2.5 px-5 py-3.5 bg-surface border border-border rounded-[10px] text-left">
          <svg className="shrink-0 w-[18px] h-[18px] mt-px text-gray" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
          </svg>
          <div className="text-xs leading-[1.6] text-gray">
            <strong className="text-dark-soft font-semibold">Teun is een AI-assistent</strong> en kan fouten maken. Antwoorden zijn bedoeld ter ondersteuning, niet als vervanging van je eigen oordeel. Controleer belangrijke informatie altijd aan de hand van de aangegeven bronnen.
          </div>
        </div>
      </div>
    </div>
  );
}
