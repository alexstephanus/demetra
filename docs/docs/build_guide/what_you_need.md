# What You Need
This section outlines everything you'll need to build the project: both parts and tools.

## Parts
### PCB
- [ ] All electronic components
- [ ] PCB + stencil

### Enclosure
- [ ] 28x M3x5x4 heat-set inserts (5mm diameter, 4mm depth)

### Final Assembly
- [ ] 28x M3x6mm screws
- [ ] 6x Peristaltic pumps
- [ ] 6x JST-XH pigtails (2-pin plug, 6" wire length)
- [ ] pH probe (BNC connector)
- [ ] ORP probe (BNC connector)
- [ ] 10k NTC thermistor (Beta value configurable)
- [ ] Conductivity probe (2-wire, K = 1.0)
- [ ] Float switch (mechanical or capacitive, required for solenoid operation)
- [ ] LCD screen (4.0" capacitive touch, model number MSP4031)
- [ ] Power supply (see Final Assembly section for specs)
- [ ] Heat-shrink tubing (optional)
- [ ] Flood sensor (optional)

#### Selecting a Power Supply
There are a couple considerations you need to keep in mind when selecting a power supply:

- **Dimensions:** The barrel jack is 2.0mm ID, 5.5mm OD, center-positive.
- **Voltage:** Demetra is designed to operate using either a 12 or 24V DC power supplies.   This voltage gets routed directly to both the peristaltic dosing pumps and configurable outlets, so you should select your power supply voltage according to what pumps you're planning on using.  Worth noting, this does mean that you're locked into one voltage for both your pumps and outlets.
- **Current:** Without any load on the outlets, Demetra requires <1A of power.  Demetra's outlets can supply a maximum of 3A of current split between them.

## Tools
### PCB Assembly
- [ ] Surface-mount PCB soldering tools (see notes below for specific recommendations)
- [ ] Soldering iron, flux, solder, and solder wick for post-reflow touchups and installing through-hole components
- [ ] 99% isopropyl alcohol for flux cleanup (highly recommended to ensure sensor reliability)
- [ ] Multimeter (for testing)
- [ ] (Optional) Conformal Coating for the sensor section

**SMD soldering tools** - the below is what I use, not a prescriptive list:
  - Low-temperature solder paste (ChipQuik NC191LT10)
  - Solder paste squeegee (I use a metal drywall putty knife)
  - ESD-safe tweezers
  - Hakko 394 vacuum pick-up tool (not strictly needed, but I do quite like it)
  - Hakko omnivise (if you do much SMD soldering I can't recommend this highly enough)
  - Controleo3 reflow oven (this is a recent addition: early revisions were soldered using a toaster oven from Goodwill and a watchful eye)

I don't recommend this approach for the sake of your sanity, but all of the components on the board are large enough to be hand-soldered, if you have an insane amount of patience.  It'll take a _lot_ longer than solder paste and a reflow oven, but it is possible to do.

### Enclosure

- [ ] 3D printer (or pre-printed enclosure)
- [ ] Soldering iron to install heat-set inserts
- [ ] M3 heat-set insert soldering iron tip (optional, but highly recommended)

### Final Assembly

- [ ] Screwdriver that fits your M3x6mm screw heads (you'll also need a small flathead bit to tighten the screw terminals)
- [ ] Wire crimping tool + ferrules (optional, but makes for more reliable connections)


## Part Purchase Links 
These aren't affiliate links, they're just here to save you some time looking around for appropriate parts.

- Filament: [Amazon](https://www.amazon.com/eSUN-Filament-Printing-Dimensional-Accuracy/dp/B0D25JMYNS)
- Heat-set inserts: [Amazon (comes with soldering iron tip)](https://www.amazon.com/dp/B0D7M3LJDL)
- Screws: [Amazon](https://www.amazon.com/dp/B0CGTRWZ12)
- Screen: [Amazon](https://www.amazon.com/dp/B0CRGQN58D) [AliExpress](https://www.aliexpress.us/item/3256807422726669.html?) (Note: You need the TOUCH option on AliExpress)
- Peristaltic pumps: [AliExpress](https://www.aliexpress.us/item/3256804283799290.html)
- JST XH Pigtails for peristaltic pumps: [Amazon](https://www.amazon.com/dp/B073SNHTFK)

There are also some optional purchases that you might find useful:

- Wire crimping tool & ferrules: [Amazon](https://www.amazon.com/dp/B0DYJG235H?th=1)
- SMA to BNC adapter (if you have SMA pH or ORP sensors): [DigiKey](https://www.digikey.com/en/products/detail/rf-solutions/ADP-SMAF-BNCM/4271257)
- BNC to screw-terminal adapter (if your EC probe has a BNC connector and you don't want to chop it off -- understandable!) [Amazon](https://www.amazon.com/QMseller-Terminal-Connector-Coaxial-Adapter/dp/B07XY5XLFV/)
