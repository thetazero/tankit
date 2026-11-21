# Setting

Decisions so far. Deliberately thin: more lore later.

## Premise

A forever war across a crowded region of some ten thousand colonized star
systems. Nobody remembers how it started; it keeps going anyway. Mostly automated, with humans still in it.

**Tone:** grim and melancholic (Dune, Horizon's old world): ruins, faded human
traces, a war that doesn't notice them. Things that behave like magic
unambiguously exist; whether they're science or magic is left **ambiguous**,
on purpose.

## Why tanks

Worlds worth fighting over sit under **planetary shields**. A shield stops
fleets, bombardment and anything large; only small things get through. So a
world can't be taken from orbit, only on the ground, by small machines dropped
in under the shield. The shields themselves are contested, along with the
rest of a world's infrastructure and its crystal. Take a world's shield and
its sky opens; until then the fight stays on the ground.

## The player

A **mercenary pilot** of the Company, remote-linked into each chassis,
working for whoever pays. Losing a chassis just drops the link; the next
contract drops you a new one. That's the in-world reason for respawns. What
the pilot is, is left deliberately vague.

## The Company

The mercenaries are a faction of their own: **the Company**, a galactic
mercenary outfit that sells its pilots to whichever side of the war is
paying. It isn't one of the war's sides and doesn't hold ground; it fights
inside other people's battles.

**In a match**, you're the one tank in the Company's colors among the hiring
side's machines: hired help, and it shows. For now only the player is
Company; later some battles may be more or less merc-heavy.

**Colors:** hazard amber over black, a working color nobody's war is fought
in. On a Company tank the amber goes over the maker's paint wherever it was
built; the maker's own fittings (gilt, glass, crystal) stay.

**The emblem:** a blade through a coin. Paid work, done with a weapon.

**One rank.** The Company ranks its pilots, and the rank decides which
contracts it trusts you with: harder tiers open as you climb. It's a number
with a title (Recruit, Hireling, Freelance, Gunhand, Veteran, Sergeant,
Lieutenant, Captain, Marshal, Warlord, five ranks to each), and there are a
lot of them: renown earned in the field (kills, time on a held site) climbs
it, slower the higher you go.

**Stolen gear.** The Company makes nothing of its own yet. Its pilots start
on the Imperium's basic kit (Kataphrakt and Kontos), the most common machine
in the galaxy, refitted in Company amber. Everything else is **stolen**: each
contract names a schematic from a side you're fighting there as its prize,
and winning it brings the schematic home. Lose or draw and you don't get it.
Once the Company has a schematic, that contract just pays crystals. A
Company tank is a set of stolen parts from different makers, which is the
mercenary's look: mixed builds are the merc identity. Company-made gear may
come later.

## Factions

Four sides at war. Any of them can hire the Company.

| Faction | Who they are |
|---|---|
| **The Imperium** | Human. A vast power fighting with automated war machines commanded from orbit. An empire that is also a religion, Byzantine in spirit. See [The Imperium](#the-imperium). |
| **The Resonant** | Human. A militant fundamentalist cult in the far reaches of human space, built around the crystals. The crystals answer: a machine with a crystal in it won't stay dead. That it happens is not in doubt; whether it's magic or science nobody understands is never resolved. See [The Resonant](#the-resonant). |
| **Uthenn** | The machines: a splinter from the aliens. See [Uthenn](#uthenn). |
| **The Ithessar** | The aliens. They shape plasma with crystal: their machines float on it, their weapons throw it. |

**Where they are.** Humans hold one part of the region and the aliens and
the machines another, and the two overlap. The densest fighting is inside
each: the Imperium against the Resonant on the human side, the Ithessar
against Uthenn on theirs. Where human and alien space meet, the fronts are
thinner.

## Emblems

Each side has an emblem, drawn in the HUD's hairlines next to its name
wherever it appears on screen (`src/ui/emblem.rs`):

- **Imperium:** a planet under a halo: a gold disc with an arch over it.
- **Resonant:** a crystal in a pointed-arch window, a relic in its case.
- **Uthenn:** a tungsten cube.
- **Ithessar:** plasma held in crystal: an orb in an almond-shaped lens.
- **The Company:** a blade through a coin.

## Factions vs. gameplay

Gameplay stays **arcade tanks**: any hull with any turret, unlocked through
play. Factions are the world, never a lock on your build. They show up as:

- **Employers**: who the contract is for. Whoever hires you, you're the
  Company's: renown goes to your one Company rank, not to the employer.
- **Schematic sources**: the parts the Company doesn't start with are stolen
  from their makers, as the prize of a mission against them. Stolen parts
  stay yours whoever hires you next.
- **Enemies**: who the bots are, how they look, and the guns they bring (each
  side's bots use its own faction's turrets).
- **Map owners**: whose territory a map is.
- **Part makers**: every hull and turret has a maker, and parts from
  different makers mix freely. Each maker has its own kind of weapon:
  - **Imperium**: conventional guns firing fast shells (Kontos, Pelekys).
  - **Ithessar**: slow plasma (Ammeth's orbs skip off walls; Ossrin's
    needles spin up into a stream).
  - **Uthenn**: long-range beams (Lumen). A continuous beam with a small
    amount of aim assist; damage falls off with range, but not too hard. It
    overheats instead of reloading. Heat is tied to the tank: bigger hulls
    and heavier turrets have more thermal mass. Cooling is a flat rate.
  - **Resonant**: the strange weapons, starting with Hymn, a censer that
    breathes short-range blue crystal fire, and Knell, a siege bell (below).
    Later maybe shields, drains, phasing.

  Hulls: the Imperium make the tracked ones (Kataphrakt, Kastron), the Ithessar
  one that hovers (Sethuun), the Resonant a vessel that won't stay dead
  (Vigil), the Uthenn one that's big, bulky and slow (Gravis): tracked, very
  sci-fi, with green accents.

## The Resonant

**They don't build tanks; they build vessels for a crystal.** The crystal
comes first: a cult finds one and builds a body to carry it, the way you'd
build a reliquary or a church. The crystal sits at the machine's heart behind
glass, like a relic in its case.

**Rites, not engineering.** They don't really know why their machines move.
They follow forms handed down: the bell goes here, the pipes run this way,
these words are carved there. It works, and nobody can say whether it's the
rite or something else.

**Each one is made by hand and named.** Built over years, carrying the marks
of everyone who worked on it. Damage is mended and the scars kept; nothing is
scrapped. **Relics:** a piece of a revered fallen machine (a bell, a plate, its
crystal) is built into a new one. That's how they honor their dead.

**The materials of worship:** bells, pipes like an organ's, stained glass of
crystal, censers, banners, carved scripture.

**They run on the crystal, visibly.** Blue light flows through the machine in
veins and pipes, and holds together parts that barely touch. It should look
like it shouldn't work.

**They won't stay dead.** A machine with a crystal in it keeps running after
it's broken: dark, cracked, blue pouring out of it, still driving and
firing, for a few moments before it finally goes. This is the faction's one
miracle in play. The Vigil is that vessel: no armor to speak of (the crystal
is its armor), fast. A narrow keel between bare tracks, its plates floating
well clear of it on shafts of light, a pointed-arch prow like a lectern, and
over the stern a gilt arch with the bell hung in it and the crystal floating
beneath, turning.
Broken, it's paid as a kill at once but runs on untouchable, at full
strength for a moment and then fading out.

**Their weapon is fire.** Hymn is a reliquary crowned with crystal, a censer
hung at its flank: purifying blue crystal fire, short range, deadly up close.

**Knell, the siege bell.** A great bronze bell lying on its side in a gilt
yoke, mouth to the enemy, a bank of organ pipes behind it and a crystal in
its crown. It rings only when the rite is complete: hold the trigger and the
pipes light one by one as it rings up, and at full it tolls by itself. Let
go early and the rite is broken; nothing happens. The toll is a ring of
crystal light and sound that throws whatever it strikes up and away, bowling
light machines over, and shoves the Knell's own hull back.

**Color:** crystal blue, in their light and on the HUD. Their machines are
worn ivory and gilt; no tank wears its side's color.

## The Imperium

**An empire that is also a religion.** Byzantine in spirit: there is an
Emperor, the state is sacred, and everything is done with ceremony that
long ago stopped needing a reason. Ritual for its own sake.

**Not the Resonant.** The Resonant worship the crystal; the Imperium worships
the Imperium. Their machines are mass-made and identical, stamped with the
emblem and sent down from orbit.

**They hire the Company** for the same reason as everyone else: they want
to win. Its pilots start on Imperium kit, so the Imperium's machines are the
ones a merc knows first.

**The emblem:** a planet under a halo: a gold disc with an arch over it.

**Color:** deep crimson and gold.

**Where the ritual shows:** on maps, subtly. Banners and cloth in crimson,
gold mosaic, small shrines, the emblem on walls and pods. Their machines
are crimson with gold light and the emblem in gilt, but otherwise clean,
plated, made in bulk.

**Names:** Byzantine flavored.

**Their cities:** Roman scale in raw concrete. Domed rotundas, colonnades,
triumphal arches over processional avenues, statues on plinths, all cast in
bulk; ordinary concrete blocks and brutalist slabs on pillars around them.
Half ruined where the war has reached them, and still hung with crimson.

## Uthenn

A splinter from the aliens, not the humans. Paperclip maximizers: what
they make is tungsten cubes. How the Ithessar feel about them is unknown;
"Uthenn" is the Ithessar word for it.

**The Foundry:** one of their tungsten cube factories, destroyed.

## Crystals

What everyone fights over: your pay, the Resonant's sacrament, and the reason
anyone fights over dead worlds. They're the currency in the arcade loop:
contracts pay them, and they buy upgrades to the parts you've stolen.

Crystal is nobody's: one substance, the same wherever it's found. Raw, in the
rock and in the ore, it's cloudy blue-grey and dull, crusted with rock dust,
only a faint cold light deep in it. Cut, it glows crystal blue, as it does at
the Resonant's sanctum. The Ithessar's machines are their own worked crystal,
near-black glass, a different thing from the ore they dig.

**The Seam:** an Ithessar crystal mine dug into a desert escarpment. The seam
crops out on the floor of a dry wash and runs up into the cliff; the Ithessar
have quarried the face back to sheer stone and followed the vein in, with a
rail adit and a pillared hall under the mesa, a new heading where a
plasma borer holds its beam on the rock day and night, and their own freight railroad
out of the adit's mouth over the wash: their combat machines float, but their
ore still goes by rail. The Resonant call the seam holy, and have dug their
own way into the rock from the east to take it back.

## Modes are contracts

- **Attrition** (team deathmatch): the Company is hired by one side and
  drops you into its battle, paid per kill. The side with more kills when the clock runs out wins.
  Most battles are two sides; some are all four at once, every side against
  every other.
- **Harvest** (instead of capture the flag): a crystal bloom surfaces; crack it,
  carry crystals to your extraction point, deny the enemy theirs. Carrying
  crystals makes you a little stronger, but they resonate, so everyone can see
  you.
- **Hold** (king of the hill): secure a crystal site, paid while you hold it.
  First on the Sanctum, a Resonant holy site: a great church on a mesa, the
  site the crystal on its altar. The Imperium have come for it.
- More later as other job types (escort, salvage, ...).

## First map

A devastated town in a desert.

## Open

- The nature of the Ithessar.
- How the war started, and what keeps it going.
- The first map's world and what happened there.
