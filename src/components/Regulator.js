import {
  Slider,
  SliderTrack,
  SliderThumb,
  SliderFilledTrack,
  Text,
  Tag,
  Stack,
  HStack,
  Box,
} from "@chakra-ui/react";
import { useState } from "react";
import IButton from "./Button";

function ISlider({ value, onChange }) {
  return (
    <Stack>
      <Slider
        defaultValue={100}
        min={50}
        max={150}
        onChange={v => onChange(v / 100)}
        orientation="vertical"
        value={value * 100}
        h="80px"
      >
        <SliderTrack bgColor="#665c54">
          <SliderFilledTrack bgColor="#98971a" />
        </SliderTrack>
        <SliderThumb bgColor="#928374" />
      </Slider>
      <Tag size="lg" bgColor="#98971a">
        <Text color="#32302f" fontWeight="bold">
          {value.toFixed(2)}
        </Text>
      </Tag>
    </Stack >
  )
}

function Regulator({ regulator, onSubmit }) {
  const [innerRegulator, setInnerRegulator] = useState(regulator);

  return (
    <HStack>
      {!!innerRegulator &&
        innerRegulator.map((v, i) =>
          <Box marginRight="5px">
            <ISlider
              key={i}
              value={v}
              onChange={v => {
                const arr = innerRegulator.concat();
                arr[i] = v;
                setInnerRegulator(arr);
              }}
            />
          </Box>
        )}
      <Stack>
        <IButton text="重置" onClick={() => setInnerRegulator(innerRegulator.map(() => 1.0))} />
        <IButton text="提交" onClick={() => onSubmit(innerRegulator)} />
      </Stack>
    </HStack>
  )
}

export default Regulator
