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
fleets and anything large; only small things get through, and slowly, at a
cost in crystal for each machine dropped through. So a world can't be taken
from orbit, only on the ground, by small machines dropped in under the
shield. The shields themselves are contested, along with the rest of a
world's infrastructure and its crystal.

A world has one or two **shield generators**. While any of them stands, an
attacker lands only a trickle. Destroy them all and the sky opens: landing is
free and the fight is even, until the defenders repair a generator, which
takes them a while.

**Bombarding a world from orbit is taboo.** Every side keeps to it, for now.

## The war

Some ten thousand colonized star systems. A world's factories build machines
on the ground, so defending is cheap; moving machines between stars is
expensive, so attacking is not. Most of the region is quiet at any one time,
but there is always war somewhere.

**Light things travel, heavy things don't.** Information, pilots and crystal
cross the stars quickly, for next to nothing. Hulls, and the basic resources an
economy runs on (not modeled in the game), don't: moving them is expensive.

**Realms, not empires.** Armies don't travel well, so no faction is run
as one state. The four factions are four peoples (a faith, a culture, a way
of building machines), and the war is fought by their **realms**: a ruler
holding a handful of systems from a seat, with its own crystal in its own
vaults. A realm goes to war for its own reasons: a neighbor's mines and
factories, when it sees a good chance of taking them. What it takes becomes
its own. Realms of one people rarely fight each other, though sometimes they
do.

No realm can rule far from its seat. Holdings too distant break away as
realms of their own, and small neighbors of one people fold together. Realms
grow by conquest, split at their edges, and merge, and nobody ever wins.

A contested system is a **theater**: its worlds, their shield generators,
mines and factories, and the machines each side holds there.

## The player

A **mercenary pilot** of the Company, remote-linked into each chassis,
working for whoever pays. Losing a chassis just drops the link, and you're
linked into the next of your company's machines in that battle. That's the
in-world reason for respawns, and it isn't free: every side brings only so
many machines to a battle, and each one lost is gone from the theater. What
the pilot is, is left deliberately vague.

## The Company

The mercenaries are a faction of their own: **the Company**, the name many
free companies fight under. It isn't one of the war's sides and keeps no
ground. Mostly it fights inside other people's battles.

**Companies are ships.** Each free company is a single ship, with its own
machines, pilots and crystal aboard. It goes where the work is and pays to
cross the stars like anyone else. Your garage is a hangar aboard your
company's ship, in orbit over the world it's contracted to fight for.

**Why realms hire it.** A realm that could use more force for a while, to take
a neighbor's world or hold its own, hires a company when the company's price
is less than what the help is worth. A company brings its own machines
and pilots. It quotes some crystal up front and so much a battle, and a realm
short of crystal owes the rest.

**Raids.** Companies are amoral. One with no work looks for a rich world too
weakly held, takes it planet by planet, empties the vaults, and leaves; the
world goes back to whoever held it. A planet's vault falls only with the whole
planet. So a realm that piles up crystal has to keep an army to guard it, or
spend it.

**Pilots** go where the pay is. Realms train them and pay them; companies bid
them away.

A company that grows too rich splits, since one ship can only work one
contract at a time, and the new one keeps its ways of doing business. One
with too little left joins another, and veterans' savings found new ones.

**Jobs.** You're a pilot for hire, like any other. You join a company for one
contract: it offers a wage a battle, and more for each kill, by how well your
gear fights, and pays it out of its own crystal. Its ship flies to the job and
you fight there until the contract's over, or you walk out on it and lose the
rest of the pay. Then you're free, still aboard, garage and all, till you sign
on with another. What the company does is none of your business; which company
you fly with is. A poor offer, or none, is the market's word on your gear.

**In a match**, your company's machines fight in the Company's colors among
the hiring side's: hired help, and it shows. Some battles have only you,
some a whole wing of amber.

**Colors:** hazard amber over black, a working color nobody's war is fought
in. On a Company tank the amber goes over the maker's paint wherever it was
built; the maker's own fittings (gilt, glass, crystal) stay.

**The emblem:** a blade through a coin. Paid work, done with a weapon.

**One rank.** The Company ranks its pilots. It's a number with a title (Recruit, Hireling, Freelance, Gunhand, Veteran, Sergeant,
Lieutenant, Captain, Marshal, Warlord, five ranks to each), and there are a
lot of them: renown earned in the field (kills, time on a held site) climbs
it, slower the higher you go.

**Gear is schematics.** The Company makes nothing of its own yet. Its pilots
start on the Imperium's basic kit (Kataphrakt and Kontos), the most common
machine in the galaxy, refitted in Company amber. Everything else is
**reverse-engineered**: destroy other people's tanks, take apart the wrecks,
and you learn to build their parts. A schematic is only information, so a
pilot's gear goes wherever the pilot does. A
Company tank is a set of parts learned from different makers, which is the
mercenary's look: mixed builds are the merc identity. Company-made gear may
come later.

## Factions

Four peoples at war, each split into many realms. Any realm can hire the Company.

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
- **Schematic sources**: the parts a pilot doesn't start with are
  reverse-engineered from their makers' wrecks. They stay yours whoever hires
  you next.
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

**Their foundries** cast hulls from tungsten cubes: pale shell on a dark
plinth, everything square, a green seam lit along it.

## Crystals

What everyone fights over: your pay, the Resonant's sacrament, and the reason
anyone fights over dead worlds. They're the currency in the arcade loop:
contracts pay them, and they buy upgrades to the parts you've learned.

Crystal is nobody's: one substance, the same wherever it's found. Raw, in the
rock and in the ore, it's cloudy blue-grey and dull, crusted with rock dust,
only a faint cold light deep in it. Cut, it glows crystal blue, as it does at
the Resonant's sanctum. The Ithessar's machines are their own worked crystal,
near-black glass, a different thing from the ore they dig.

**The crystal mines:** Ithessar mines dug into desert escarpments, each one
its own (the maps are generated: no two the same). The seam crops out on the
floor of a dry wash and runs up into the cliff; the Ithessar
have quarried the face back to sheer stone and followed the vein in, with a
rail adit and a pillared hall under the mesa, a new heading where a
plasma borer holds its beam on the rock day and night, and their own freight railroad
out of the adit's mouth over the wash: their combat machines float, but their
ore still goes by rail. The Resonant call the seam holy, and have dug their
own way into the rock from the east to take it back. (The first of them,
the Seam, was the one hand-built mine; the generated ones are its kind.)

The others' mines are the same ground, worked their own way: a site's mine is
in its founder's style. The Imperium cuts with a thermal lance on a crawler,
by caged amber lamps, and hauls behind a crimson diesel. The Resonant bring a
reliquary whose choir of pipes sings the rock apart in crystal light, and haul
in chapels on wheels. The Uthenn hold a Lumen's beam on it out of a stack of
cubes. Whoever's it is, driving into the beam destroys you.

**The hull foundries:** where machines are made, on the ground they'll be
fought over. Furnace halls, mills, casting sheds and a power house on rolling ground, roads
and a railway between them over bridges and through tunnels, on desert, tundra
or grassland (generated: no two the same). Each people builds its own: the Imperium's raw concrete and sawtooth
roofs hung with crimson; the Resonant's ivory halls under slate, a crystal
burning in each furnace; the Uthenn's stacks of cubes; the Ithessar's dark
stone, plasma held between crystals.

**The rail depots:** outposts on desert worlds, where freight waits for a
train: sidings side by side under gantries, water towers at their ends, a
main line off into the sand. Each people builds its own, out of what it builds
its foundries of.

## Modes are contracts

- **Attrition** (a raid on an enemy camp): no ground changes hands; the job
  is to cost the enemy machines. Paid per kill.
  Every battle is two sides.
- **Raid** (one-sided capture the flag): one side guards a flag, the other
  has to carry it off. First in the crystal mines.
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
