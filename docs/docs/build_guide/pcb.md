# PCB Assembly

This section covers populating and reflowing the motherboard, as well as attaching through-hole components. It doesn't get into the nuts and bolts of SMD soldering.  If you haven't done this before, it is totally possible to learn! You should watch a few tutorials at a minimum and, additionally, consider using any of the litany of surface-mount practice boards to familiarize yourself with the process before diving into assembling this motherboard.

## Before You Start

**Required Tools:**

- [ ] Surface-mount soldering equipment (see [What You Need](what_you_need.md))
- [ ] Soldering iron, flux, and solder wick
- [ ] 99% isopropyl alcohol for cleanup
- [ ] Multimeter for testing
- [ ] Magnification device (Magnifier visor, stereo microscope, or magnifying lamp) to accurately place components

**Required Parts:**

- [ ] Bare PCB
- [ ] Solder paste stencil
- [ ] Electronic Components


## SMD Component Assembly

### **Note:**
You shouldn't leave un-reflowed solder paste on the PCB.  So, if you're applying solder paste with a stencil, make sure you have enough time to populate all components in one go.  This takes me about 3 hours by hand, and I know where everything goes.  So, be advised, it'll take a while. You can deal with THT components later, in multiple batches, or one a day for a month if you so desire.

### 1. Apply Solder Paste

- [ ] Align stencil over PCB
- [ ] Apply solder paste with squeegee
- [ ] Inspect paste application quality

### 2. Place Components

- [ ] Populate all surface-mount components.

**Important: Verify Polarized Component Orientation**

Before reflowing, double-check the orientation of these polarized components:

- [ ] Diodes (verify cathode markings are opposite the empty side of the rectangle)
- [ ] Electrolytic bulk capacitor (C153) should line up with the silkscreen marking
- [ ] ICs (verify pin 1 indicators match PCB silkscreen)

Also, you should ensure that there aren't any bare SMD pads.  If there are, go back and add the appropriate component there.

### 3. Reflow
- [ ] Place PCB in reflow oven.
- [ ] Run appropriate reflow profile for the solder paste you're using (Or wing it, if you're just using a toaster oven)
- [ ] Allow PCB to cool before handling.

### 4. Inspection for Bridging
Using magnification, carefully inspect all components, paying special attention to fine-pitch components:

**Components requiring extra attention:**

- [ ] U103: Current-sensing ADC
- [ ] U202: ESP32
- [ ] J52: Micro-USB connector
- [ ] J61: Screen connector
- [ ] U8, U9, U10: Analog switches for temperature & EC sensors

**Check for:**

- [ ] Solder bridges between pins
- [ ] Cold solder joints
- [ ] Tombstoned components
- [ ] Missing components

### 5. Rework (if needed)
- [ ] Apply flux to any bridged pins
- [ ] Use solder wick to remove excess solder
- [ ] Reflow individual components as needed

Once you're satisfied with your reflow quality, it's time to install through-hole components.

## Through-Hole Component Assembly

**Note:** Some through-hole components are installed on top of the PCB (The side you just populated).  All of the external connectors on the bottom of the PCB are installed on the underside of the PCB.

### 1. Install Through-Hole Components on Top of the PCB

Components to install:

- [ ] J55: CAN bus terminal
- [ ] J58: RS485 terminal
- [ ] J101-106: JST XH Connectors
- [ ] J51: Serial pin header
- [ ] K151: Relay
- [ ] J52: Micro-USB connector (the component should already be populated, but you have to solder the 3 through-hole pins to keep it in place.  The surface-mount pads alone won't stand up to unplugging a cable -- if you skip this step you'll rip the connector, and probably the pads, off of the board when you try to use it)

### 2. Install Through-Hole Components on Underside of PCB

**Note: Be precise here**
The tolerances on the enclosure openings are pretty tight, so you don't have a ton of wiggle room when soldering these connectors.  Ensure they're flush with the PCB and not askew.  It might help to solder a single pin of each connector, then insert the PCB into the enclosure, soldering the other pins once you're sure everything fits well.

- [ ] J107-110: Outlet terminals
- [ ] J151: DC Barrel Jack
- [ ] J1, J4: 2-pin EC & temperature sensor pluggable terminals
- [ ] J2, J3: BNC Connectors
- [ ] J59, J62: 3-pin Digital IO and Float Switch pluggable terminals
- [ ] J60: 4-pin external I2C pluggable terminal

### 3. Final Cleanup
- [ ] Clean any remaining flux with isopropyl alcohol, paying particular attention to the sensor sections.

## Visual Inspection Checklist

- [ ] All components placed and oriented correctly
- [ ] No solder bridges visible under magnification
- [ ] No cold solder joints
- [ ] All flux residue cleaned
- [ ] No damaged components

## Electrical Testing

Before proceeding to powering it up and flashing the firmware, perform some basic electrical tests:

### Power Supply Tests
- [ ] Using a multimeter, check for shorts between power and ground rails (should read open circuit)
  - [ ] Main power nets:
    - [ ] DC Input: the rectangular pad of the DC power jack (farthest from the edge of the board) is V_DC
    - [ ] 3.3V: This via is located on J201, just below the ESP32
    - [ ] 9V: This pin is located on J62
    - [ ] Main GND
  - [ ] Isolated sensor nets
    - [ ] pH
      - [ ] 3.3V: + via adjacent to U15
      - [ ] GND: - via adjacent to U15
      - [ ] 1V: This via is just above the pH BNC connector, near R1
    - [ ] ORP
      - [ ] 3.3V: + via just above the ORP BNC connector, to the left of U2
      - [ ] GND: - via just above the ORP BNC connector, to the left of U2
      - [ ] 1V: This via is just below C31, check for shorts here between the + and - vias
    - [ ] EC/Temperature
      - [ ] 3.3V: + via adjacent to U13
      - [ ] GND: - via adjacent to U13
      - [ ] 1V: The 1V via is just above the temperature sensor terminal block.

Once you're satisfied with a visual inspection and you've verified that there are no shorts on power rails, you can proceed to flashing the firmware and assembling the unit.

## Next Steps

Once the PCB is fully assembled and tested, proceed to [Enclosure Assembly](enclosure.md).
